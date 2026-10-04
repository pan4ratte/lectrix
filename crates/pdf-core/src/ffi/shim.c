/*
 * Lectrix FFI shim: MuPDF calls the `mupdf` crate does not wrap.
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

typedef struct lectrix_error {
	int code;
	char message[256];
} lectrix_error;

static int lectrix_caught(fz_context *ctx, lectrix_error *err)
{
	err->code = fz_caught(ctx);
	fz_strlcpy(err->message, fz_caught_message(ctx), sizeof err->message);
	/* Never 0, so callers can test for success with `== 0`. */
	return err->code ? err->code : -1;
}

const char *lectrix_mupdf_version(void)
{
	return FZ_VERSION;
}

/*
 * 1 if these headers match the linked library. fz_new_context passes the header
 * FZ_VERSION, and the library refuses (returns NULL) when it differs from its own.
 */
int lectrix_mupdf_headers_match_library(void)
{
	fz_context *ctx = fz_new_context(NULL, NULL, FZ_STORE_UNLIMITED);
	if (!ctx)
		return 0;
	fz_drop_context(ctx);
	return 1;
}

int lectrix_pdf_enable_journal(fz_context *ctx, pdf_document *doc, lectrix_error *err)
{
	fz_try(ctx)
		pdf_enable_journal(ctx, doc);
	fz_catch(ctx)
		return lectrix_caught(ctx, err);
	return 0;
}

int lectrix_pdf_undo(fz_context *ctx, pdf_document *doc, lectrix_error *err)
{
	fz_try(ctx)
		pdf_undo(ctx, doc);
	fz_catch(ctx)
		return lectrix_caught(ctx, err);
	return 0;
}

int lectrix_pdf_redo(fz_context *ctx, pdf_document *doc, lectrix_error *err)
{
	fz_try(ctx)
		pdf_redo(ctx, doc);
	fz_catch(ctx)
		return lectrix_caught(ctx, err);
	return 0;
}

/*
 * Starts an operation with no name. Its changes are folded into the previous journal
 * step when it ends, or kept out of the history when there is none: they are not an undo
 * step of their own. End it with pdf_end_operation.
 */
int lectrix_pdf_begin_implicit_operation(fz_context *ctx, pdf_document *doc, lectrix_error *err)
{
	fz_try(ctx)
		pdf_begin_implicit_operation(ctx, doc);
	fz_catch(ctx)
		return lectrix_caught(ctx, err);
	return 0;
}

/* Writes the current position (0 = original document) and the step count. */
int lectrix_pdf_undoredo_state(fz_context *ctx, pdf_document *doc, int *current, int *steps, lectrix_error *err)
{
	fz_try(ctx)
		*current = pdf_undoredo_state(ctx, doc, steps);
	fz_catch(ctx)
		return lectrix_caught(ctx, err);
	return 0;
}

/* Copies the name of journal step `step` into `buf` (empty if the step has no name). */
int lectrix_pdf_undoredo_step(fz_context *ctx, pdf_document *doc, int step, char *buf, size_t len, lectrix_error *err)
{
	fz_try(ctx)
	{
		const char *name = pdf_undoredo_step(ctx, doc, step);
		fz_strlcpy(buf, name ? name : "", len);
	}
	fz_catch(ctx)
		return lectrix_caught(ctx, err);
	return 0;
}

/*
 * Sets the data of stream object `num` to `data` (raw: still encoded, so its /Filter stays
 * valid), like pdf_update_stream(..., compressed = 1). The object must have been created
 * in the journal operation in progress, or the document must have no journal.
 *
 * MuPDF records the first change to each object in an operation by scanning every change
 * already recorded in it, so copying thousands of objects in one undo step is quadratic
 * (inserting a 2,881-page book took minutes). An object created in the operation needs no
 * record of later changes: undo removes it whole, with its stream. So the journal is set
 * aside for this one write.
 */
int lectrix_pdf_set_new_stream(fz_context *ctx, pdf_document *doc, int num, const unsigned char *data, size_t len, lectrix_error *err)
{
	pdf_journal *journal = doc->journal;
	fz_buffer *buf = NULL;
	pdf_obj *ref = NULL;
	fz_var(buf);
	fz_var(ref);
	doc->journal = NULL;
	fz_try(ctx)
	{
		buf = fz_new_buffer_from_copied_data(ctx, data, len);
		ref = pdf_new_indirect(ctx, doc, num, 0);
		pdf_update_stream(ctx, doc, ref, buf, 1);
	}
	fz_always(ctx)
	{
		doc->journal = journal;
		pdf_drop_obj(ctx, ref);
		fz_drop_buffer(ctx, buf);
	}
	fz_catch(ctx)
		return lectrix_caught(ctx, err);
	return 0;
}

/*
 * Asks MuPDF to write a new appearance stream for `annot` at its next pdf_update_annot.
 * Without this, an annotation that has no appearance only gets a local one for display,
 * which is never saved (repair needs a saved one, AGENTS.md section 5.3).
 */
int lectrix_pdf_dirty_annot(fz_context *ctx, pdf_annot *annot, lectrix_error *err)
{
	fz_try(ctx)
		pdf_dirty_annot(ctx, annot);
	fz_catch(ctx)
		return lectrix_caught(ctx, err);
	return 0;
}

/*
 * A read-only fz_stream over an operating-system file handle that the caller opened.
 *
 * MuPDF's own file stream (fz_open_file) opens files without FILE_SHARE_DELETE on
 * Windows, so the file cannot be replaced while the document is open. Lectrix opens the
 * handle itself with read and delete sharing (crates/pdf-core/src/ffi/stream.rs), which
 * lets an atomic save rename the new file over the open one (ADR 0003). The callbacks
 * mirror MuPDF's file stream (source/fitz/stream-open.c).
 */
typedef struct lectrix_os_stream
{
#ifdef _WIN32
	HANDLE handle;
#else
	int fd;
#endif
	unsigned char buffer[65536];
} lectrix_os_stream;

static int lectrix_os_next(fz_context *ctx, fz_stream *stm, size_t max)
{
	lectrix_os_stream *state = stm->state;
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
static void lectrix_os_seek(fz_context *ctx, fz_stream *stm, int64_t offset, int whence)
{
	lectrix_os_stream *state = stm->state;
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

static void lectrix_os_close(intptr_t handle)
{
#ifdef _WIN32
	CloseHandle((HANDLE)handle);
#else
	close((int)handle);
#endif
}

static void lectrix_os_drop(fz_context *ctx, void *state_)
{
	lectrix_os_stream *state = state_;
#ifdef _WIN32
	lectrix_os_close((intptr_t)state->handle);
#else
	lectrix_os_close((intptr_t)state->fd);
#endif
	fz_free(ctx, state);
}

/*
 * Opens a PDF from `handle` (a Windows HANDLE or a POSIX file descriptor, opened for
 * reading). Ownership of the handle always passes to this function: it is closed when
 * the document is dropped, or here on failure.
 */
int lectrix_pdf_open_os_handle(fz_context *ctx, intptr_t handle, pdf_document **out, lectrix_error *err)
{
	lectrix_os_stream *state = NULL;
	fz_stream *stm = NULL;
	int stream_owns_handle = 0;
	fz_var(state);
	fz_var(stm);
	fz_var(stream_owns_handle);
	*out = NULL;
	fz_try(ctx)
	{
		state = fz_malloc_struct(ctx, lectrix_os_stream);
#ifdef _WIN32
		state->handle = (HANDLE)handle;
#else
		state->fd = (int)handle;
#endif
		/* From here on the state (and the handle) is released by lectrix_os_drop, even if
		 * fz_new_stream fails. */
		stream_owns_handle = 1;
		stm = fz_new_stream(ctx, state, lectrix_os_next, lectrix_os_drop);
		stm->seek = lectrix_os_seek;
		*out = pdf_open_document_with_stream(ctx, stm);
	}
	fz_always(ctx)
		fz_drop_stream(ctx, stm); /* the document keeps its own reference */
	fz_catch(ctx)
	{
		if (!stream_owns_handle)
			lectrix_os_close(handle);
		return lectrix_caught(ctx, err);
	}
	return 0;
}

/* 1 if MuPDF had to repair the file's cross-reference table when opening it. */
int lectrix_pdf_was_repaired(fz_context *ctx, pdf_document *doc, int *repaired, lectrix_error *err)
{
	fz_try(ctx)
		*repaired = pdf_was_repaired(ctx, doc);
	fz_catch(ctx)
		return lectrix_caught(ctx, err);
	return 0;
}


/*
 * Crash recovery (AGENTS.md section 7). A snapshot is the document's file followed by its
 * unsaved changes as an incremental section, written without finalizing that section in
 * memory, so the document carries on as if nothing was written (undo history included).
 * The path is UTF-8; MuPDF converts it for Windows (fz_fopen_utf8).
 *
 * MuPDF's saved journals (pdf_save_journal / pdf_load_journal) would bring the undo
 * history back too, but 1.27.2 cannot load a journal that records any change:
 * pdf_deserialise_journal links its entries into the history while
 * pdf_add_journal_fragment only appends to a pending operation, so the first change throws
 * "Can't add a journal fragment absent an operation". ADR 0001 has the details.
 */
int lectrix_pdf_save_snapshot(fz_context *ctx, pdf_document *doc, const char *path, lectrix_error *err)
{
	fz_try(ctx)
		pdf_save_snapshot(ctx, doc, path);
	fz_catch(ctx)
		return lectrix_caught(ctx, err);
	return 0;
}

/*
 * Drops never-used entries from the end of the document's edit section (the in-memory
 * incremental xref) and returns how many it dropped.
 *
 * Undoing a step marks the objects it created as never used (type 0), but they stay in
 * the section's length. An incremental save writes that length as the trailer's /Size
 * while writing none of those entries, so /Size runs past the highest object in the file
 * and qpdf warns. Never-used entries at the end carry nothing (no object, no stream), and
 * the section never shrinks below the older sections, so the file says the same with or
 * without them. If a redo or a new object needs them again, MuPDF grows the section back,
 * zero-filled (pdf_get_incremental_xref_entry, resize_xref_sub in pdf-xref.c).
 *
 * Nothing here can throw: it only reads and shortens MuPDF's own structures (public in
 * pdf/document.h and pdf/xref.h, checked against the library at startup).
 */
int lectrix_pdf_trim_unused_objects(pdf_document *doc)
{
	pdf_xref *xref;
	pdf_xref_subsec *sub;
	int floor = 0;
	int n, i;

	/* Only the edit section, outside any operation and when viewing the latest version. */
	if (doc->num_incremental_sections == 0 || doc->xref_base != 0 || doc->local_xref_nesting > 0)
		return 0;
	xref = &doc->xref_sections[0];
	sub = xref->subsec;
	if (sub == NULL || sub->next != NULL || sub->start != 0 || sub->len != xref->num_objects)
		return 0;
	for (i = 1; i < doc->num_xref_sections; i++)
		if (doc->xref_sections[i].num_objects > floor)
			floor = doc->xref_sections[i].num_objects;

	n = xref->num_objects;
	while (n > floor && sub->table[n - 1].type == 0 && sub->table[n - 1].obj == NULL && sub->table[n - 1].stm_buf == NULL)
		n--;
	i = xref->num_objects - n;
	sub->len = n;
	xref->num_objects = n;
	return i;
}

/* 1 if the document has changes that a save would write. */
int lectrix_pdf_has_unsaved_changes(fz_context *ctx, pdf_document *doc, int *changed, lectrix_error *err)
{
	fz_try(ctx)
		*changed = pdf_has_unsaved_changes(ctx, doc);
	fz_catch(ctx)
		return lectrix_caught(ctx, err);
	return 0;
}
