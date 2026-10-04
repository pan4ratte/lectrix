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
