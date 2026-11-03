/*
 * Images in the document exported (section 6.14, ADR 0035): the MuPDF calls behind
 * crates/pdf-core/src/export/embedded.rs. An image is loaded by its object number; this
 * file says what decides whether its own bytes can be copied into a file, hands over those
 * bytes and its colour profile, and decodes its pixels with its soft or stencil mask as
 * alpha. The decisions are made in Rust. Nothing in the document is changed.
 *
 * Every function catches MuPDF's errors (fz_try), so its longjmp never crosses a Rust frame.
 */
#include <string.h>
#include "mupdf/fitz.h"
#include "mupdf/pdf.h"
#include "shim.h"

typedef struct
{
	/* The size a PNG of it has (image_size). */
	int w, h, bpc, n;
	int imagemask;
	/* A soft mask or a stencil mask (an image), which MuPDF keeps as image->mask. */
	int has_mask;
	int use_decode;
	int use_colorkey;
	/* /Decode is [1 0] for every component. */
	int decode_inverted;
	/* The image's colour space, as enum fz_colorspace_type (0, none, for a mask). */
	int colorspace;
	/* Its bytes as the file has them: 0 none to copy, 1 JPEG, 2 JPEG 2000. */
	int compressed;
	/* A JPEG's /ColorTransform, -1 if the file doesn't say. */
	int color_transform;
	size_t compressed_len;
} lectrix_image_facts;

static void image_size(fz_image *image, int max_side, int *w, int *h);

/* Loads image object `num` of `doc`; lectrix_image_drop lets it go. */
int lectrix_image_load(fz_context *ctx, pdf_document *doc, int num, fz_image **out, lectrix_error *err)
{
	pdf_obj *ref = NULL;

	*out = NULL;
	fz_var(ref);
	fz_try(ctx)
	{
		ref = pdf_new_indirect(ctx, doc, num, 0);
		if (!pdf_is_stream(ctx, ref) || !pdf_name_eq(ctx, pdf_dict_get(ctx, ref, PDF_NAME(Subtype)), PDF_NAME(Image)))
			fz_throw(ctx, FZ_ERROR_ARGUMENT, "object %d is not an image", num);
		*out = pdf_load_image(ctx, doc, ref);
	}
	fz_always(ctx)
		pdf_drop_obj(ctx, ref);
	fz_catch(ctx)
		return lectrix_caught(ctx, err);
	return 0;
}

void lectrix_image_drop(fz_context *ctx, fz_image *image)
{
	fz_drop_image(ctx, image);
}

void lectrix_image_facts_of(fz_context *ctx, fz_image *image, lectrix_image_facts *facts)
{
	fz_compressed_buffer *cbuf = fz_compressed_image_buffer(ctx, image);
	int i, type = cbuf ? cbuf->params.type : FZ_IMAGE_UNKNOWN;
	unsigned char *data;

	memset(facts, 0, sizeof *facts);
	/* The size it is written at as PNG: its mask's, if that is finer. */
	image_size(image, 0, &facts->w, &facts->h);
	facts->bpc = image->bpc;
	facts->n = image->n;
	facts->imagemask = image->imagemask;
	facts->has_mask = image->mask != NULL;
	facts->use_decode = image->use_decode;
	facts->use_colorkey = image->use_colorkey;
	facts->decode_inverted = image->n > 0;
	for (i = 0; i < image->n && i < FZ_MAX_COLORS; i++)
		if (image->decode[2 * i] != 1 || image->decode[2 * i + 1] != 0)
			facts->decode_inverted = 0;
	facts->colorspace = image->colorspace ? (int)fz_colorspace_type(ctx, image->colorspace) : FZ_COLORSPACE_NONE;
	facts->color_transform = -1;
	if (type == FZ_IMAGE_JPEG)
	{
		facts->compressed = 1;
		facts->color_transform = cbuf->params.u.jpeg.color_transform;
	}
	else if (type == FZ_IMAGE_JPX)
		facts->compressed = 2;
	if (facts->compressed && cbuf->buffer)
		facts->compressed_len = fz_buffer_storage(ctx, cbuf->buffer, &data);
	else
		facts->compressed = 0;
}

/*
 * The image's JPEG or JPEG 2000 data, as the file has it once the filters before that
 * one are undone; NULL if it has none. Owned by the image, alive as long as it is.
 */
const unsigned char *lectrix_image_compressed(fz_context *ctx, fz_image *image, size_t *len)
{
	fz_compressed_buffer *cbuf = fz_compressed_image_buffer(ctx, image);
	unsigned char *data = NULL;

	*len = 0;
	if (!cbuf || !cbuf->buffer)
		return NULL;
	if (cbuf->params.type != FZ_IMAGE_JPEG && cbuf->params.type != FZ_IMAGE_JPX)
		return NULL;
	*len = fz_buffer_storage(ctx, cbuf->buffer, &data);
	return data;
}

/*
 * The colour profile of the image's colour space (an Indexed one's base), if the document
 * gives one of its own (ICCBased); NULL for MuPDF's device spaces and for spaces without
 * one. Owned by the image, alive as long as it is.
 */
const unsigned char *lectrix_image_icc(fz_context *ctx, fz_image *image, size_t *len)
{
	fz_colorspace *cs = image->colorspace;
	unsigned char *data = NULL;

	*len = 0;
	if (cs && cs->type == FZ_COLORSPACE_INDEXED)
		cs = cs->u.indexed.base;
	if (!cs || !(cs->flags & FZ_COLORSPACE_IS_ICC) || !cs->u.icc.buffer)
		return NULL;
	if (cs == fz_device_gray(ctx) || cs == fz_device_rgb(ctx) || cs == fz_device_bgr(ctx) || cs == fz_device_cmyk(ctx))
		return NULL;
	*len = fz_buffer_storage(ctx, cs->u.icc.buffer, &data);
	return data;
}

/* The image's pixels at w x h: decoded at its own size, or, when that is larger, at a
 * power of two at or above w x h (JPEG and JPEG 2000 decode smaller themselves); then
 * scaled to w x h, up or down. */
static fz_pixmap *image_pixmap(fz_context *ctx, fz_image *image, int w, int h)
{
	fz_pixmap *pix, *scaled = NULL;
	fz_matrix ctm;

	fz_var(scaled);
	if (image->w <= w && image->h <= h)
		pix = fz_get_pixmap_from_image(ctx, image, NULL, NULL, NULL, NULL);
	else
	{
		ctm = fz_scale(w, h);
		pix = fz_get_pixmap_from_image(ctx, image, NULL, &ctm, NULL, NULL);
	}
	if (pix->w == w && pix->h == h)
		return pix;
	fz_try(ctx)
		scaled = fz_scale_pixmap(ctx, pix, 0, 0, w, h, NULL);
	fz_always(ctx)
		fz_drop_pixmap(ctx, pix);
	fz_catch(ctx)
		fz_rethrow(ctx);
	if (!scaled || scaled->w != w || scaled->h != h)
	{
		fz_drop_pixmap(ctx, scaled);
		fz_throw(ctx, FZ_ERROR_ARGUMENT, "an image could not be scaled");
	}
	return scaled;
}

/* The size the image is written at: its own, or its mask's where the mask has more pixels
 * (a scan's text, sharp, over colours kept coarse); with `max_side`, at most that a side. */
static void image_size(fz_image *image, int max_side, int *w, int *h)
{
	*w = image->w;
	*h = image->h;
	if (image->mask && !image->imagemask && (double)image->mask->w * image->mask->h > (double)image->w * image->h)
	{
		*w = image->mask->w;
		*h = image->mask->h;
	}
	if (max_side > 0 && (*w > max_side || *h > max_side))
	{
		if (*w >= *h)
		{
			*h = (int)((float)*h * max_side / *w + 0.5f);
			*w = max_side;
		}
		else
		{
			*w = (int)((float)*w * max_side / *h + 0.5f);
			*h = max_side;
		}
	}
	if (*w < 1)
		*w = 1;
	if (*h < 1)
		*h = 1;
}

/*
 * Decodes the image: in grey or RGB (CMYK, Lab, separations and the like converted to RGB,
 * an Indexed space expanded), its soft or stencil mask as alpha, premultiplied as MuPDF
 * keeps alpha, at the size of whichever of the two has more pixels (image_size). A
 * stencil mask (/ImageMask) comes out as alpha alone, 255 where it paints. With
 * `max_side`, at most that many pixels a side, for a thumbnail.
 *
 * It reads only the image, whose data MuPDF loaded with it, so it may run on another
 * thread than the document's, with that thread's own context (as display lists are drawn).
 */
int lectrix_image_pixels(fz_context *ctx, fz_image *image, int max_side, fz_pixmap **out, lectrix_error *err)
{
	fz_pixmap *pix = NULL, *mask = NULL, *tmp = NULL;
	int w, h;
	enum fz_colorspace_type type;

	*out = NULL;
	image_size(image, max_side, &w, &h);
	fz_var(pix);
	fz_var(mask);
	fz_var(tmp);
	fz_try(ctx)
	{
		pix = image_pixmap(ctx, image, w, h);
		if (!image->imagemask && pix->colorspace)
		{
			type = fz_colorspace_type(ctx, pix->colorspace);
			if ((type != FZ_COLORSPACE_GRAY && type != FZ_COLORSPACE_RGB) || pix->s)
			{
				tmp = fz_convert_pixmap(ctx, pix, fz_device_rgb(ctx), NULL, NULL, fz_default_color_params, 1);
				fz_drop_pixmap(ctx, pix);
				pix = tmp;
				tmp = NULL;
			}
		}
		if (image->mask && !image->imagemask && !pix->alpha)
		{
			mask = image_pixmap(ctx, image->mask, w, h);
			if (mask->n == 1)
			{
				tmp = fz_new_pixmap_from_color_and_mask(ctx, pix, mask);
				fz_drop_pixmap(ctx, pix);
				pix = tmp;
				tmp = NULL;
			}
		}
		*out = pix;
		pix = NULL;
	}
	fz_always(ctx)
	{
		fz_drop_pixmap(ctx, mask);
		fz_drop_pixmap(ctx, tmp);
		fz_drop_pixmap(ctx, pix);
	}
	fz_catch(ctx)
		return lectrix_caught(ctx, err);
	return 0;
}
