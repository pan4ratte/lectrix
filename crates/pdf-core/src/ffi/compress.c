/*
 * Compress (section 6.11, ADR 0027): the MuPDF and libjpeg calls behind
 * crates/pdf-core/src/compress/. The decisions are made in Rust; this file finds where
 * each image is drawn, decodes images, encodes new ones and subsets fonts.
 *
 * Every function catches MuPDF's errors (fz_try) and libjpeg's (setjmp), so neither
 * longjmp ever crosses a Rust frame.
 */
#include <setjmp.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <math.h>
#include "mupdf/fitz.h"
#include "mupdf/pdf.h"
#include "jpeglib.h"
#include "shim.h"

/* ---------------------------------------------------------------------------------------
 * Where images are drawn.
 *
 * A content-stream processor that keeps the transformation matrix through q, Q and cm, and
 * follows it into form XObjects (their /Matrix), tiling patterns (their /Matrix, from the
 * space of the stream that uses them), soft-mask groups and annotation appearances. For
 * each image XObject drawn it reports the image's object number and its resolution there,
 * in pixels per inch of the page (the smaller of the two axes), and the same for the image's
 * soft mask and stencil mask. MuPDF's own image rewriter measures images inside a form in
 * the form's space instead, which is not the page's whenever the form is scaled.
 *
 * Inline images and images inside Type 3 glyphs are not objects that can be replaced, and
 * are not reported. Nothing in the document is changed.
 */

typedef void (*lectrix_image_found)(void *opaque, int num, float dpi);

typedef struct
{
	pdf_processor super;
	pdf_document *doc;
	lectrix_image_found found;
	void *opaque;
	fz_matrix ctm;
	/* The space of the content stream being read: the page's, or a form's. */
	fz_matrix base;
	fz_matrix *stack;
	int depth;
	int cap;
	/* q/Q inside a form never pop below the form's own start. */
	int floor;
	int nesting;
	/* How many more forms, patterns and soft masks this page may enter. */
	int budget;
} lectrix_image_walk;

#define LECTRIX_WALK_NESTING 32
#define LECTRIX_WALK_BUDGET 100000

static void walk_push(fz_context *ctx, lectrix_image_walk *w)
{
	if (w->depth == w->cap)
	{
		int cap = w->cap ? w->cap * 2 : 32;
		w->stack = fz_realloc_array(ctx, w->stack, cap, fz_matrix);
		w->cap = cap;
	}
	w->stack[w->depth++] = w->ctm;
}

static void walk_q(fz_context *ctx, pdf_processor *proc)
{
	walk_push(ctx, (lectrix_image_walk *)proc);
}

static void walk_Q(fz_context *ctx, pdf_processor *proc)
{
	lectrix_image_walk *w = (lectrix_image_walk *)proc;
	if (w->depth > w->floor)
		w->ctm = w->stack[--w->depth];
}

static void walk_cm(fz_context *ctx, pdf_processor *proc, float a, float b, float c, float d, float e, float f)
{
	lectrix_image_walk *w = (lectrix_image_walk *)proc;
	w->ctm = fz_concat(fz_make_matrix(a, b, c, d, e, f), w->ctm);
}

/* Pixels per inch of an image `width` x `height` drawn with `ctm` (in points), along its
 * less detailed axis; 0 if it is drawn with no size. */
static float walk_dpi(fz_matrix ctm, int width, int height)
{
	float x = sqrtf(ctm.a * ctm.a + ctm.b * ctm.b);
	float y = sqrtf(ctm.c * ctm.c + ctm.d * ctm.d);
	float dpix, dpiy;
	if (x <= 0 || y <= 0)
		return 0;
	dpix = width * 72.0f / x;
	dpiy = height * 72.0f / y;
	return dpix < dpiy ? dpix : dpiy;
}

static void walk_report_one(fz_context *ctx, lectrix_image_walk *w, pdf_obj *obj)
{
	int width, height;
	float dpi;
	if (!pdf_is_indirect(ctx, obj) || !pdf_is_stream(ctx, obj))
		return;
	width = pdf_dict_get_int(ctx, obj, PDF_NAME(Width));
	height = pdf_dict_get_int(ctx, obj, PDF_NAME(Height));
	if (width <= 0 || height <= 0)
		return;
	dpi = walk_dpi(w->ctm, width, height);
	if (dpi > 0)
		w->found(w->opaque, pdf_to_num(ctx, obj), dpi);
}

static void walk_Do_image(fz_context *ctx, pdf_processor *proc, const char *name, fz_image *image)
{
	lectrix_image_walk *w = (lectrix_image_walk *)proc;
	pdf_obj *obj = pdf_lookup_resource(ctx, proc->rstack, PDF_NAME(XObject), name);
	if (!obj)
		return;
	walk_report_one(ctx, w, obj);
	walk_report_one(ctx, w, pdf_dict_get(ctx, obj, PDF_NAME(SMask)));
	walk_report_one(ctx, w, pdf_dict_get(ctx, obj, PDF_NAME(Mask)));
}

/* Reads `stm` (with `res`, or the current resources if it has none) drawn with `matrix`
 * in the space `from`. Errors in it are ignored: the rest of the page is still read. */
static void walk_stream(fz_context *ctx, lectrix_image_walk *w, pdf_obj *stm, pdf_obj *res, fz_matrix matrix, fz_matrix from)
{
	int depth = w->depth;
	int floor = w->floor;
	fz_matrix ctm = w->ctm;
	fz_matrix base = w->base;

	if (!stm || w->nesting >= LECTRIX_WALK_NESTING || w->budget <= 0)
		return;
	w->nesting++;
	w->budget--;
	fz_try(ctx)
	{
		walk_push(ctx, w);
		w->floor = w->depth;
		w->ctm = fz_concat(matrix, from);
		w->base = w->ctm;
		pdf_process_contents(ctx, &w->super, w->doc, res, stm, NULL, NULL);
	}
	fz_always(ctx)
	{
		w->depth = depth;
		w->floor = floor;
		w->ctm = ctm;
		w->base = base;
		w->nesting--;
	}
	fz_catch(ctx)
	{
		fz_rethrow_if(ctx, FZ_ERROR_SYSTEM);
		fz_ignore_error(ctx);
	}
}

static void walk_Do_form(fz_context *ctx, pdf_processor *proc, const char *name, pdf_obj *form)
{
	lectrix_image_walk *w = (lectrix_image_walk *)proc;
	walk_stream(ctx, w, form, pdf_xobject_resources(ctx, form), pdf_xobject_matrix(ctx, form), w->ctm);
}

static void walk_gs_SMask(fz_context *ctx, pdf_processor *proc, pdf_obj *smask, fz_colorspace *smask_cs, float *bc, int luminosity, pdf_obj *tr)
{
	lectrix_image_walk *w = (lectrix_image_walk *)proc;
	if (smask)
		walk_stream(ctx, w, smask, pdf_xobject_resources(ctx, smask), pdf_xobject_matrix(ctx, smask), w->ctm);
}

/* A tiling pattern's /Matrix maps it into the space of the stream that uses it. */
static void walk_pattern(fz_context *ctx, lectrix_image_walk *w, pdf_pattern *pat)
{
	if (pat && pat->contents)
		walk_stream(ctx, w, pat->contents, pat->resources, pat->matrix, w->base);
}

static void walk_sc_pattern(fz_context *ctx, pdf_processor *proc, const char *name, pdf_pattern *pat, int n, float *color)
{
	walk_pattern(ctx, (lectrix_image_walk *)proc, pat);
}

static void walk_SC_pattern(fz_context *ctx, pdf_processor *proc, const char *name, pdf_pattern *pat, int n, float *color)
{
	walk_pattern(ctx, (lectrix_image_walk *)proc, pat);
}

/*
 * Calls `found` for each image drawn on page `page_index`: by its contents, its annotations'
 * appearances and its widgets'. An image drawn several times is reported each time.
 */
int lectrix_walk_page_images(fz_context *ctx, pdf_document *doc, int page_index, lectrix_image_found found, void *opaque, lectrix_error *err)
{
	pdf_page *page = NULL;
	lectrix_image_walk *w = NULL;
	pdf_annot *annot;
	pdf_obj *resources;
	float unit;

	fz_var(page);
	fz_var(w);
	fz_try(ctx)
	{
		page = pdf_load_page(ctx, doc, page_index);
		w = pdf_new_processor(ctx, sizeof *w);
		w->super.requirements = 0;
		w->super.op_q = walk_q;
		w->super.op_Q = walk_Q;
		w->super.op_cm = walk_cm;
		w->super.op_Do_image = walk_Do_image;
		w->super.op_Do_form = walk_Do_form;
		w->super.op_gs_SMask = walk_gs_SMask;
		w->super.op_sc_pattern = walk_sc_pattern;
		w->super.op_SC_pattern = walk_SC_pattern;
		w->doc = doc;
		w->found = found;
		w->opaque = opaque;
		w->budget = LECTRIX_WALK_BUDGET;
		/* Lengths in points: /UserUnit says how many points a unit of user space is. */
		unit = pdf_dict_get_real_default(ctx, page->obj, PDF_NAME(UserUnit), 1);
		if (unit <= 0)
			unit = 1;
		w->ctm = fz_scale(unit, unit);
		w->base = w->ctm;

		resources = pdf_page_resources(ctx, page);
		pdf_process_contents(ctx, &w->super, doc, resources, pdf_page_contents(ctx, page), NULL, NULL);
		pdf_processor_push_resources(ctx, &w->super, resources);
		for (annot = pdf_first_annot(ctx, page); annot; annot = pdf_next_annot(ctx, annot))
			pdf_process_annot(ctx, &w->super, annot, NULL);
		for (annot = pdf_first_widget(ctx, page); annot; annot = pdf_next_widget(ctx, annot))
			pdf_process_annot(ctx, &w->super, annot, NULL);
		pdf_close_processor(ctx, &w->super);
	}
	fz_always(ctx)
	{
		if (w)
		{
			fz_free(ctx, w->stack);
			w->super.closed = 1;
			pdf_drop_processor(ctx, &w->super);
		}
		if (page)
			fz_drop_page(ctx, &page->super);
	}
	fz_catch(ctx)
		return lectrix_caught(ctx, err);
	return 0;
}

/* ---------------------------------------------------------------------------------------
 * Decoding.
 */

/*
 * Decodes image object `num` into a pixmap of its own size, with its /Decode applied and
 * an Indexed colour space expanded to its base. An image mask comes out as MuPDF draws
 * it: 255 where it paints.
 */
int lectrix_image_decode(fz_context *ctx, pdf_document *doc, int num, fz_pixmap **out, lectrix_error *err)
{
	pdf_obj *ref = NULL;
	fz_image *image = NULL;

	*out = NULL;
	fz_var(ref);
	fz_var(image);
	fz_try(ctx)
	{
		ref = pdf_new_indirect(ctx, doc, num, 0);
		image = pdf_load_image(ctx, doc, ref);
		*out = fz_get_pixmap_from_image(ctx, image, NULL, NULL, NULL, NULL);
	}
	fz_always(ctx)
	{
		fz_drop_image(ctx, image);
		pdf_drop_obj(ctx, ref);
	}
	fz_catch(ctx)
		return lectrix_caught(ctx, err);
	return 0;
}

/* The colour space of `pix`: 0 none, 1 grey, 2 RGB, 3 CMYK, 4 anything else. */
int lectrix_pixmap_info(fz_context *ctx, const fz_pixmap *pix, int *w, int *h, int *n, int *alpha, ptrdiff_t *stride, unsigned char **samples)
{
	fz_colorspace *cs = pix->colorspace;
	*w = pix->w;
	*h = pix->h;
	*n = pix->n;
	*alpha = pix->alpha;
	*stride = pix->stride;
	*samples = pix->samples;
	if (pix->s)
		return 4;
	if (!cs)
		return 0;
	if (fz_colorspace_is_gray(ctx, cs))
		return 1;
	if (fz_colorspace_is_rgb(ctx, cs))
		return 2;
	if (fz_colorspace_is_cmyk(ctx, cs))
		return 3;
	return 4;
}

/* `pix` in device grey, through the colour management MuPDF draws with. */
int lectrix_pixmap_to_gray(fz_context *ctx, fz_pixmap *pix, fz_pixmap **out, lectrix_error *err)
{
	*out = NULL;
	fz_try(ctx)
		*out = fz_convert_pixmap(ctx, pix, fz_device_gray(ctx), NULL, NULL, fz_default_color_params, 0);
	fz_catch(ctx)
		return lectrix_caught(ctx, err);
	return 0;
}

void lectrix_drop_pixmap(fz_context *ctx, fz_pixmap *pix)
{
	fz_drop_pixmap(ctx, pix);
}

/* ---------------------------------------------------------------------------------------
 * Encoding. Each encoder hands back memory from malloc, which lectrix_free releases.
 */

void lectrix_free(void *p)
{
	free(p);
}

static int lectrix_out_of_memory(lectrix_error *err)
{
	err->code = -1;
	fz_strlcpy(err->message, "out of memory", sizeof err->message);
	return -1;
}

/* Copies `len` bytes of `data` into memory from malloc. */
static int copy_out(const unsigned char *data, size_t len, unsigned char **out, size_t *out_len, lectrix_error *err)
{
	*out = malloc(len ? len : 1);
	if (!*out)
		return lectrix_out_of_memory(err);
	memcpy(*out, data, len);
	*out_len = len;
	return 0;
}

typedef struct
{
	struct jpeg_error_mgr pub;
	jmp_buf jump;
	char message[JMSG_LENGTH_MAX];
} lectrix_jpeg_error;

static void lectrix_jpeg_error_exit(j_common_ptr cinfo)
{
	lectrix_jpeg_error *e = (lectrix_jpeg_error *)cinfo->err;
	(*cinfo->err->format_message)(cinfo, e->message);
	longjmp(e->jump, 1);
}

/* libjpeg's warnings would go to stderr. */
static void lectrix_jpeg_quiet(j_common_ptr cinfo, int level)
{
	(void)cinfo;
	(void)level;
}

typedef struct
{
	unsigned char *data;
	unsigned long len;
} lectrix_jpeg_dest;

/*
 * Encodes `samples` (`n` = 1, 3 or 4 components: grey, RGB or CMYK, 8 bits each) as a
 * baseline JPEG at `quality` (1-100). MozJPEG's encoder (the libjpeg MuPDF links, ADR 0024)
 * with trellis quantization and optimized Huffman tables, but not its default progressive
 * mode: every reader decodes a baseline JPEG, and faster. `subsample` halves the colour
 * resolution of an RGB image both ways (4:2:0); otherwise 4:4:4. CMYK is written as it
 * is, with the Adobe marker libjpeg writes for it, as PDF files hold it.
 */
int lectrix_encode_jpeg(const unsigned char *samples, int w, int h, int n, ptrdiff_t stride, int quality, int subsample, unsigned char **out, size_t *out_len, lectrix_error *err)
{
	struct jpeg_compress_struct cinfo;
	lectrix_jpeg_error jerr;
	lectrix_jpeg_dest dest = { NULL, 0 };
	JSAMPROW row;

	*out = NULL;
	*out_len = 0;
	if (n != 1 && n != 3 && n != 4)
	{
		err->code = -1;
		fz_strlcpy(err->message, "JPEG needs 1, 3 or 4 components", sizeof err->message);
		return -1;
	}
	memset(&cinfo, 0, sizeof cinfo);
	cinfo.err = jpeg_std_error(&jerr.pub);
	jerr.pub.error_exit = lectrix_jpeg_error_exit;
	jerr.pub.emit_message = lectrix_jpeg_quiet;
	jerr.message[0] = 0;
	if (setjmp(jerr.jump))
	{
		jpeg_destroy_compress(&cinfo);
		free(dest.data);
		err->code = -1;
		fz_strlcpy(err->message, jerr.message[0] ? jerr.message : "JPEG encoding failed", sizeof err->message);
		return -1;
	}
	jpeg_create_compress(&cinfo);
	jpeg_mem_dest(&cinfo, &dest.data, &dest.len);
	cinfo.image_width = (JDIMENSION)w;
	cinfo.image_height = (JDIMENSION)h;
	cinfo.input_components = n;
	cinfo.in_color_space = n == 1 ? JCS_GRAYSCALE : n == 3 ? JCS_RGB : JCS_CMYK;
	jpeg_set_defaults(&cinfo);
	/* MozJPEG's defaults are progressive; this makes it sequential (baseline). */
	jpeg_c_set_bool_param(&cinfo, JBOOLEAN_OPTIMIZE_SCANS, FALSE);
	cinfo.num_scans = 0;
	cinfo.scan_info = NULL;
	cinfo.optimize_coding = TRUE;
	jpeg_set_quality(&cinfo, quality < 1 ? 1 : quality > 100 ? 100 : quality, TRUE);
	if (n == 3)
	{
		cinfo.comp_info[0].h_samp_factor = subsample ? 2 : 1;
		cinfo.comp_info[0].v_samp_factor = subsample ? 2 : 1;
		cinfo.comp_info[1].h_samp_factor = 1;
		cinfo.comp_info[1].v_samp_factor = 1;
		cinfo.comp_info[2].h_samp_factor = 1;
		cinfo.comp_info[2].v_samp_factor = 1;
	}
	jpeg_start_compress(&cinfo, TRUE);
	while (cinfo.next_scanline < cinfo.image_height)
	{
		row = (JSAMPROW)(samples + (ptrdiff_t)cinfo.next_scanline * stride);
		jpeg_write_scanlines(&cinfo, &row, 1);
	}
	jpeg_finish_compress(&cinfo);
	jpeg_destroy_compress(&cinfo);
	*out = dest.data;
	*out_len = dest.len;
	return 0;
}

/*
 * Encodes `samples` (`n` = 1, 3 or 4 components) as JPEG 2000 through OpenJPEG.
 * MuPDF compresses at (100 - quality) : 1, and losslessly at 100 or when the image is
 * smaller than two of its tiles.
 */
int lectrix_encode_jpx(fz_context *ctx, const unsigned char *samples, int w, int h, int n, ptrdiff_t stride, int quality, unsigned char **out, size_t *out_len, lectrix_error *err)
{
	fz_pixmap *pix = NULL;
	fz_buffer *buf = NULL;
	fz_output *output = NULL;
	fz_colorspace *cs;
	int rc = 0;

	*out = NULL;
	*out_len = 0;
	fz_var(pix);
	fz_var(buf);
	fz_var(output);
	fz_try(ctx)
	{
		if (n == 1)
			cs = fz_device_gray(ctx);
		else if (n == 3)
			cs = fz_device_rgb(ctx);
		else if (n == 4)
			cs = fz_device_cmyk(ctx);
		else
			fz_throw(ctx, FZ_ERROR_ARGUMENT, "JPEG 2000 needs 1, 3 or 4 components");
		pix = fz_new_pixmap_with_data(ctx, cs, w, h, NULL, 0, (int)stride, (unsigned char *)samples);
		buf = fz_new_buffer(ctx, 4096);
		output = fz_new_output_with_buffer(ctx, buf);
		fz_write_pixmap_as_jpx(ctx, output, pix, quality);
		fz_close_output(ctx, output);
	}
	fz_always(ctx)
	{
		fz_drop_output(ctx, output);
		fz_drop_pixmap(ctx, pix);
	}
	fz_catch(ctx)
	{
		fz_drop_buffer(ctx, buf);
		return lectrix_caught(ctx, err);
	}
	rc = copy_out(buf->data, buf->len, out, out_len, err);
	fz_drop_buffer(ctx, buf);
	return rc;
}

/*
 * Encodes a 1-bit image (rows of `stride` bytes, the first pixel in the high bit, 0 for
 * black) as CCITT Group 4, to be decoded with /BlackIs1 false, which gives the bits back
 * as they are.
 */
int lectrix_encode_g4(fz_context *ctx, const unsigned char *bits, int w, int h, ptrdiff_t stride, unsigned char **out, size_t *out_len, lectrix_error *err)
{
	fz_buffer *buf = NULL;
	int rc;

	*out = NULL;
	*out_len = 0;
	fz_try(ctx)
		buf = fz_compress_ccitt_fax_g4(ctx, bits, w, h, stride);
	fz_catch(ctx)
		return lectrix_caught(ctx, err);
	rc = copy_out(buf->data, buf->len, out, out_len, err);
	fz_drop_buffer(ctx, buf);
	return rc;
}

/* Compresses `data` with zlib (Flate) at `level` (1-9). */
int lectrix_deflate(fz_context *ctx, const unsigned char *data, size_t len, int level, unsigned char **out, size_t *out_len, lectrix_error *err)
{
	unsigned char *dest = NULL;
	size_t dest_len = 0;

	*out = NULL;
	*out_len = 0;
	fz_try(ctx)
	{
		dest_len = fz_deflate_bound(ctx, len);
		dest = malloc(dest_len ? dest_len : 1);
		if (!dest)
			fz_throw(ctx, FZ_ERROR_SYSTEM, "out of memory");
		fz_deflate(ctx, dest, &dest_len, data, len, (fz_deflate_level)level);
	}
	fz_catch(ctx)
	{
		free(dest);
		return lectrix_caught(ctx, err);
	}
	*out = dest;
	*out_len = dest_len;
	return 0;
}

/* ---------------------------------------------------------------------------------------
 * Fonts.
 */

/*
 * Subsets the embedded TrueType and CFF fonts of `doc` to the glyphs its pages, annotations
 * and widgets show (MuPDF's pdf_subset_fonts). A font it cannot subset is left as it was.
 */
int lectrix_pdf_subset_fonts(fz_context *ctx, pdf_document *doc, lectrix_error *err)
{
	fz_try(ctx)
		pdf_subset_fonts(ctx, doc, 0, NULL);
	fz_catch(ctx)
		return lectrix_caught(ctx, err);
	return 0;
}
