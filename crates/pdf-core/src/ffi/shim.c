/*
 * Folio FFI shim: MuPDF calls the `mupdf` crate does not wrap.
 *
 * MuPDF reports errors with setjmp/longjmp. A longjmp must never unwind through Rust
 * frames, so every call that can throw is wrapped here in fz_try/fz_catch and turned
 * into a return code plus a copied message.
 */
#include <stdint.h>
#include <string.h>
#ifdef _WIN32
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#else
#include <errno.h>
#include <unistd.h>
#endif
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

/*
 * A read-only fz_stream over an operating-system file handle that the caller opened.
 *
 * MuPDF's own file stream (fz_open_file) opens files without FILE_SHARE_DELETE on
 * Windows, so the file cannot be replaced while the document is open. Folio opens the
 * handle itself with read and delete sharing (crates/pdf-core/src/ffi/stream.rs), which
 * lets an atomic save rename the new file over the open one (ADR 0003). The callbacks
 * mirror MuPDF's file stream (source/fitz/stream-open.c).
 */
typedef struct folio_os_stream
{
#ifdef _WIN32
	HANDLE handle;
#else
	int fd;
#endif
	unsigned char buffer[65536];
} folio_os_stream;

static int folio_os_next(fz_context *ctx, fz_stream *stm, size_t max)
{
	folio_os_stream *state = stm->state;
	size_t n;
	(void)max; /* only a hint */
#ifdef _WIN32
	DWORD got = 0;
	if (!ReadFile(state->handle, state->buffer, (DWORD)sizeof state->buffer, &got, NULL))
		fz_throw(ctx, FZ_ERROR_SYSTEM, "read error (Windows error %lu)", GetLastError());
	n = got;
#else
	ssize_t got;
	do
		got = read(state->fd, state->buffer, sizeof state->buffer);
	while (got < 0 && errno == EINTR);
	if (got < 0)
		fz_throw(ctx, FZ_ERROR_SYSTEM, "read error: %s", strerror(errno));
	n = (size_t)got;
#endif
	stm->rp = state->buffer;
	stm->wp = state->buffer + n;
	stm->pos += (int64_t)n;
	if (n == 0)
		return EOF;
	return *stm->rp++;
}

/* fz_seek has already turned SEEK_CUR into SEEK_SET, so whence is 0 or 2 here. */
static void folio_os_seek(fz_context *ctx, fz_stream *stm, int64_t offset, int whence)
{
	folio_os_stream *state = stm->state;
#ifdef _WIN32
	LARGE_INTEGER distance, position;
	DWORD method = whence == SEEK_END ? FILE_END : whence == SEEK_CUR ? FILE_CURRENT : FILE_BEGIN;
	distance.QuadPart = offset;
	if (!SetFilePointerEx(state->handle, distance, &position, method))
		fz_throw(ctx, FZ_ERROR_SYSTEM, "cannot seek (Windows error %lu)", GetLastError());
	stm->pos = position.QuadPart;
#else
	off_t position = lseek(state->fd, (off_t)offset, whence);
	if (position < 0)
		fz_throw(ctx, FZ_ERROR_SYSTEM, "cannot seek: %s", strerror(errno));
	stm->pos = (int64_t)position;
#endif
	stm->rp = state->buffer;
	stm->wp = state->buffer;
}

static void folio_os_close(intptr_t handle)
{
#ifdef _WIN32
	CloseHandle((HANDLE)handle);
#else
	close((int)handle);
#endif
}

static void folio_os_drop(fz_context *ctx, void *state_)
{
	folio_os_stream *state = state_;
#ifdef _WIN32
	folio_os_close((intptr_t)state->handle);
#else
	folio_os_close((intptr_t)state->fd);
#endif
	fz_free(ctx, state);
}

/*
 * Opens a PDF from `handle` (a Windows HANDLE or a POSIX file descriptor, opened for
 * reading). Ownership of the handle always passes to this function: it is closed when
 * the document is dropped, or here on failure.
 */
int folio_pdf_open_os_handle(fz_context *ctx, intptr_t handle, pdf_document **out, folio_error *err)
{
	folio_os_stream *state = NULL;
	fz_stream *stm = NULL;
	int stream_owns_handle = 0;
	fz_var(state);
	fz_var(stm);
	fz_var(stream_owns_handle);
	*out = NULL;
	fz_try(ctx)
	{
		state = fz_malloc_struct(ctx, folio_os_stream);
#ifdef _WIN32
		state->handle = (HANDLE)handle;
#else
		state->fd = (int)handle;
#endif
		/* From here on the state (and the handle) is released by folio_os_drop, even if
		 * fz_new_stream fails. */
		stream_owns_handle = 1;
		stm = fz_new_stream(ctx, state, folio_os_next, folio_os_drop);
		stm->seek = folio_os_seek;
		*out = pdf_open_document_with_stream(ctx, stm);
	}
	fz_always(ctx)
		fz_drop_stream(ctx, stm); /* the document keeps its own reference */
	fz_catch(ctx)
	{
		if (!stream_owns_handle)
			folio_os_close(handle);
		return folio_caught(ctx, err);
	}
	return 0;
}

/* 1 if MuPDF had to repair the file's cross-reference table when opening it. */
int folio_pdf_was_repaired(fz_context *ctx, pdf_document *doc, int *repaired, folio_error *err)
{
	fz_try(ctx)
		*repaired = pdf_was_repaired(ctx, doc);
	fz_catch(ctx)
		return folio_caught(ctx, err);
	return 0;
}

