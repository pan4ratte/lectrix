# 0003: Opening files so they can be saved in place; reopening after every save

- Status: accepted (Phase 1)
- Date: 2026-10-03

## Context

Phase 0 found that saving over the file a document was opened from fails on Windows
(`docs/progress.md`, finding 4). MuPDF's `fz_open_file` opens the file through the C
runtime without `FILE_SHARE_DELETE`, so the final step of the atomic save (AGENTS.md
section 7: rename the temporary file over the original) is refused with a sharing
violation while the document is open.

There is a second, related problem. After an incremental save, MuPDF's in-memory document
still describes the *old* file: its incremental xref sections and offsets are relative to
the bytes it opened. A second incremental save from the same in-memory document would
append to the new file with offsets that no longer match (verified in
`source/pdf/pdf-write.c`: `do_pdf_save_document` rewrites every incremental section from
the document's own `xref_base`, and nothing re-bases them onto the saved file).

## Decision

1. **Open through our own file stream.** `pdf-core/src/ffi/stream.rs` opens the file in
   Rust with `FILE_SHARE_READ | FILE_SHARE_DELETE` (no write sharing) and passes the OS
   handle to a small `fz_stream` implementation in the C shim (`lectrix_pdf_open_os_handle`,
   modelled on MuPDF's own file stream). Then `pdf_open_document_with_stream`.
   - Delete sharing lets the atomic save rename the new file over the open one. The open
     handle keeps reading the old file's data until the document is dropped (tested:
     `ffi::stream::tests::open_file_can_be_replaced_and_still_read`).
   - Without write sharing, no other program can change the bytes under MuPDF in place.
     A program that saves by rename (most do) still can, and Lectrix notices the change
     through its file watch.
   - The shim uses `ReadFile`/`SetFilePointerEx` on Windows and `read`/`lseek` elsewhere,
     behind `#ifdef _WIN32`. This is the only platform-specific code in `pdf-core`; it is
     a portability shim for the C library rather than a Windows feature, so it lives in
     `ffi/` rather than the app's `platform` module.
   - The vendored crate gains one accessor, `PdfDocument::from_raw_owned`
     (`third_party/mupdf-rs/LECTRIX_PATCHES.md`, patch 3).
2. **Reopen after every successful save.** The document actor saves through `save_atomic`,
   then opens the saved file again and replaces its in-memory document. Later
   incremental saves then append to the right bytes (tested:
   `saves_in_place_repeatedly_while_open`). If the reopen fails, the file is still saved,
   and the session falls back to a full save next time.
3. **Consequence: the undo history starts again after each save.** MuPDF's journal belongs
   to the in-memory document and cannot be carried across a reopen. Acrobat also clears
   its undo stack on save. Revisions (used for image caching and dirty state) carry over,
   so nothing is re-rendered.

## A save that fails (2026-10-11)

The write to the temporary file can succeed and the replace still fail (the target is
locked by another program, or a check before the replace refuses). The document in memory
then has to be exactly as it was, because the user saves again. MuPDF does not leave it
so, in three ways, each found from a damaged file or a test:

1. **Incremental save.** `pdf_save_document` with `do_incremental` moves the document on
   to the update it wrote: `doc->startxref` becomes that update's cross-reference offset
   (`writexref`, `writexrefstream`). The next incremental save then wrote an update whose
   `/Prev` was its own offset. Readers stop at such a loop: MuPDF read that last section
   alone, found no catalog, and opened the file with no pages and no error. A user's
   29 MB book was damaged this way on 2026-10-08, by saving again after "the file is
   open in another program". Fix: incremental saves are written with `do_snapshot`
   (`lectrix_pdf_save_incremental` in the shim), which leaves `startxref` alone and
   removes the cross-reference stream's object again; `/ID` is still renewed. After a
   save that succeeds the session reopens the file anyway (above), so nothing needs the
   document moved on.
2. **The undo history.** MuPDF wraps a save in an operation, "Save document". With a
   cross-reference stream (most files other programs write), the stream's object is
   created and deleted inside it, so the operation stayed in the journal as a step. That
   also happened for every recovery copy (a snapshot): Undo undid "Save document", nothing,
   while the session counted the user's last edit as undone. Fix: the journal is set
   aside for these writes (`lectrix_save_untouched`), and the object number the stream
   took is trimmed.
3. **Save As (optimized).** Garbage collection renumbers and merges the document's objects
   in memory. After a failed optimized save, the next incremental save lost changes. Fix:
   the session writes the document whole to a file beside the target, opens that, and
   optimizes the copy (`Actor::save_optimized`). Writing a file whole changes nothing in
   the document (tested).

Files already damaged by (1) open again: a document that opens with no pages gets its
cross-reference table rebuilt from the objects (`pdf_repair_xref`, what MuPDF's own
on-demand repair calls), which brings back everything in the file, the last update's
changes included. It then counts as repaired, so its next save is a full one.

Tests: `journal_and_save::a_save_after_one_that_failed_writes_a_sound_file`,
`journal_and_save::an_incremental_save_after_a_failed_whole_save_is_sound`,
`session::a_failed_optimized_save_leaves_the_session_as_it_was`,
`session::a_file_whose_last_section_follows_itself_is_repaired_on_open`,
`recovery::copies_of_a_file_with_a_cross_reference_stream_add_nothing_to_undo`.

`do_snapshot` is marked "do not use directly" in MuPDF's header; the snapshot functions
have no variant that renews `/ID`. The shim's comment lists what the flag does in 1.27.2;
check it again when MuPDF is upgraded.

## Hazard recorded for later

MuPDF writes output files with the C runtime's `fopen`, which creates **inheritable**
handles on Windows. A child process started while MuPDF has a file open for writing
inherits that handle and keeps the file open until it exits, which blocks the rename. The
`pdf-core` test suite hit this (a `qpdf --check` spawned by one test held another test's
temporary file), and `tests/common/mod.rs` now serializes MuPDF writes against child
processes. Lectrix itself starts no child processes. If a future feature does (for example
"Show in folder"), it must not do so while a save is in progress, or must restrict handle
inheritance.

## Alternatives considered

- **Open the whole file into memory** (`PdfDocument::from_bytes`). Simple and needs no
  shim, but a 500 MB scanned book would cost 500 MB of RAM, far over the 200 MB target
  (ADR 0002).
- **Close the document, rename, reopen.** If the rename fails (the file is locked by
  Acrobat, section 7), the unsaved edits are gone, unless we reopen from the temporary
  file and juggle paths. The share-delete stream keeps the edits safe in memory until the
  rename has succeeded.
- **Keep the in-memory document after saving and always save in full afterwards.**
  This keeps undo across saves, but breaks the "default save is incremental" rule
  (section 5.4) after the first save, and it breaks signed documents.
- **Persist the journal across the reopen** (MuPDF can write a journal to a file and read
  it back, checking a document fingerprint). The fingerprint changes when the file
  changes, so this does not fit. We can revisit if users miss undo after saving.
