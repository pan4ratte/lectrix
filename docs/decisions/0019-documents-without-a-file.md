# 0019: Documents that are not files yet

- Status: accepted (built at the user's request, 2026-10-13; the choices below were made
  while building and are the user's to change)
- Date: 2026-10-13

## Context

Extracting pages (ADR 0018, "Extracting") asked where to save the new file before it
copied anything. The user asked for the extracted pages to open as a temporary, unsaved
document in a new tab instead, with its pages as a grid, so that they can be put in order
first and saved once they are right.

Until now every document in a tab was a file: the session reads it, saves onto it, the
registry tells tabs apart by it, watches it, and keeps a recent-files entry and a
remembered view for it. A document that is not a file yet has to fit into all of that.

## Findings

- A session reads its document from a file and has to keep reading from it: MuPDF loads
  objects on demand, and an incremental save appends to those bytes (ADR 0003). A
  document held in memory only would need another way to open, save and write recovery
  copies.
- A session can already differ from "its file" from the start: one restored after a crash
  reads a recovery copy, belongs to the original file, and is unsaved until saved
  (`Origin::restored` in `pdf-core/src/session.rs`). Its first revision is at no journal
  position, so undoing everything leaves it unsaved.
- `Session::save` with a target writes the whole document there, opens that file and
  belongs to it from then on (ADR 0003). Nothing of the file it read before is needed
  after that.

## Decision

1. **The new document is a working copy on disk**, written where Lectrix keeps such
   documents: `unsaved/` in the app's local data folder, beside `recovery/`. Each gets a
   folder of its own there, named by the time, the process and a count, and the file in
   it has the name the document shows: "Pages from report.pdf". Measurement and test runs
   without a data folder (LECTRIX_EPHEMERAL) use a folder of the process in the system's
   temporary files.
2. **The session is opened as one that differs from its file from the start**
   (`Session::open_unsaved`), the way a restored one is: unsaved until it is saved to
   another file, also after undoing every change. So the tab has its unsaved dot from
   the start, and closing it asks, with no new state in the session.
3. **The registry knows which documents have no file** (`OpenDoc::unsaved` in
   `src-tauri/src/documents.rs`) and tells the frontend (`DocumentInfo::untitled`). For
   such a document:
   - Save is Save As: the frontend asks where (`app.save`), and Rust refuses a save onto
     the working copy. The dialog starts with the document's name, in the folder of the
     document its pages came from.
   - Once saved, it is that file: a recent file, watched, saved in place from then on.
     The working copy and its folder are deleted.
   - Until then it is not a recent file, has no remembered view, is not watched, and
     cannot be reloaded.
   - Closing it deletes the working copy.
4. **Crash recovery covers it like any unsaved document.** Its recovery copy names the
   working copy as its file. Restored, a document whose file is under `unsaved/` is one
   without a file again; its Save dialog then starts wherever the system puts it, since
   the folder its pages came from is not kept.
5. **Working copies nobody needs are swept**: at the start, those an earlier run left
   that no recovery copy waiting to be restored belongs to; after recovered changes are
   discarded, theirs; at a normal exit, those of the documents still open (the user chose
   not to save them).
6. **The name is not shortened for the user**, but the working copy's file name is cut
   to 96 characters, since it lies under the data folder, whose path is long already.

## Consequences

- Extracting writes the pages to disk once (the working copy) and saving writes them
  again, where before they were written once. Both are plain full saves of the extracted
  pages.
- A document that was never saved takes disk space in the app's data folder until it is
  saved or closed.
- A crash within the two minutes before the first recovery copy loses the extracted
  document, as it loses any unsaved change; the next start sweeps its working copy.
- "Pages from report.pdf" extracted from again gives "Pages from Pages from report.pdf".
- The dialog that asked where to save, and the refusals that went with it (the
  document's own file, a file open in a tab), are gone from extracting: Save As has them.

## Alternatives considered

- **Keep asking for a file first.** What the user asked to change.
- **A document in memory only.** MuPDF can open a document from a buffer, but the
  session, saving, recovery copies and the reload after a save are all built on a file
  to read (ADR 0003), and the copied pages of a large scan would stay in memory.
- **Write the working copy next to the source document**, under the suggested name.
  That puts a file the user did not ask for into their folder, and may replace one.
- **Reuse `Session::restore` with a path that does not exist yet.** Save would then
  write to that path without the user having chosen it.

## Tests

`crates/pdf-core/tests/recovery.rs` (a session opened this way is unsaved until saved to
a file, and no longer reads the working copy afterwards), `src-tauri/src/documents.rs`
(the registry: no recent file, no watching, Save refused onto the working copy, Save As,
closing, a crash and its restore, the sweeps), and `tests/e2e/specs/organize.test.mjs`
(extract, reorder, save, and closing without saving).
