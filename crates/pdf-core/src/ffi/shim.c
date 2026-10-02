/*
 * Folio FFI shim: MuPDF calls the `mupdf` crate does not wrap.
 *
 * MuPDF reports errors with setjmp/longjmp. A longjmp must never unwind through Rust
 * frames, so every call that can throw is wrapped here in fz_try/fz_catch and turned
 * into a return code plus a copied message.
 */
#include <string.h>
#include "mupdf/fitz.h"
#include "mupdf/pdf.h"

typedef struct folio_error {
	int code;
	char message[256];
} folio_error;

static int folio_caught(fz_context *ctx, folio_error *err)
{
	err->code = fz_caught(ctx);
	fz_strlcpy(err->message, fz_caught_message(ctx), sizeof err->message);
	/* Never 0, so callers can test for success with `== 0`. */
	return err->code ? err->code : -1;
}

const char *folio_mupdf_version(void)
{
	return FZ_VERSION;
}

/*
 * 1 if these headers match the linked library. fz_new_context passes the header
 * FZ_VERSION, and the library refuses (returns NULL) when it differs from its own.
 */
int folio_mupdf_headers_match_library(void)
{
	fz_context *ctx = fz_new_context(NULL, NULL, FZ_STORE_UNLIMITED);
	if (!ctx)
		return 0;
	fz_drop_context(ctx);
	return 1;
}

int folio_pdf_enable_journal(fz_context *ctx, pdf_document *doc, folio_error *err)
{
	fz_try(ctx)
		pdf_enable_journal(ctx, doc);
	fz_catch(ctx)
		return folio_caught(ctx, err);
	return 0;
}

int folio_pdf_undo(fz_context *ctx, pdf_document *doc, folio_error *err)
{
	fz_try(ctx)
		pdf_undo(ctx, doc);
	fz_catch(ctx)
		return folio_caught(ctx, err);
	return 0;
}

int folio_pdf_redo(fz_context *ctx, pdf_document *doc, folio_error *err)
{
	fz_try(ctx)
		pdf_redo(ctx, doc);
	fz_catch(ctx)
		return folio_caught(ctx, err);
	return 0;
}

/* Writes the current position (0 = original document) and the step count. */
int folio_pdf_undoredo_state(fz_context *ctx, pdf_document *doc, int *current, int *steps, folio_error *err)
{
	fz_try(ctx)
		*current = pdf_undoredo_state(ctx, doc, steps);
	fz_catch(ctx)
		return folio_caught(ctx, err);
	return 0;
}

/* Copies the name of journal step `step` into `buf` (empty if the step has no name). */
int folio_pdf_undoredo_step(fz_context *ctx, pdf_document *doc, int step, char *buf, size_t len, folio_error *err)
{
	fz_try(ctx)
	{
		const char *name = pdf_undoredo_step(ctx, doc, step);
		fz_strlcpy(buf, name ? name : "", len);
	}
	fz_catch(ctx)
		return folio_caught(ctx, err);
	return 0;
}

/*
 * Copies `count` pages of `src` (indices in `pages`) into `dst`, inserting them at
 * `insert_at` (or appending when negative). One graft map is shared by all pages, so
 * resources the pages share (fonts, images) are copied once.
 */
int folio_pdf_graft_pages(fz_context *ctx, pdf_document *dst, int insert_at, pdf_document *src, const int *pages, int count, folio_error *err)
{
	pdf_graft_map *map = NULL;
	int i;
	fz_var(map);
	fz_try(ctx)
	{
		map = pdf_new_graft_map(ctx, dst);
		for (i = 0; i < count; i++)
			pdf_graft_mapped_page(ctx, map, insert_at < 0 ? -1 : insert_at + i, src, pages[i]);
	}
	fz_always(ctx)
		pdf_drop_graft_map(ctx, map);
	fz_catch(ctx)
		return folio_caught(ctx, err);
	return 0;
}
