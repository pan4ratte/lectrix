# Document model, undo/redo and saving

Part of the Lectrix specification. [AGENTS.md](../AGENTS.md) holds the rules every change follows, with section 7 in short; this file is section 7 in full, and keeps its numbers, so a reference to it still leads here.

The Rust session is the single source of truth: the frontend holds only a view of it, and the undo history lives in Rust.

**Operations.** Every change is a variant of one `Operation` enum in `ops.rs` (for example `AddAnnotation`, `UpdateAnnotation`, `DeleteAnnotation`, `AddBookmark`, `SetPageLabels`, `RotatePages`, `MovePages`, `DeletePages`, `InsertPages`, `RepairAnnotations`). Applying an operation increments the document revision and returns what changed. The session reads again only what the operation can have changed: the whole outline, which takes a tenth of a second in a file with thousands of bookmarks, only after bookmark edits and after pages are deleted or inserted (`OutlineEffect` in `ops.rs`, ADR 0018).

**Undo/redo.** Uses MuPDF's built-in journalling, wrapped in `ffi/journal.rs`; each `Operation` is one journal step with a human-readable name ("Add highlight", "Rename bookmark"). The Edit menu shows the name of the step being undone or redone. Saving reopens the file, so undo history starts again after each save (ADR 0003); after a crash restore it starts at the restored state (MuPDF cannot load a saved journal, ADR 0001).

**Dirty state.** A document is dirty when its revision differs from the last saved revision. Show a dot in the tab (a spinning ring while saving; there is no status bar), and prompt when the tab closes, when its window closes and on app exit.

**Documents without a file** (ADR 0019). Pages extracted into a new document (section 6.7) open as a document that is no file yet. Its tab has the name it would be saved under ("Pages from report.pdf"), the unsaved dot from the start, and "Not saved yet" as its tooltip. Save asks where to save it, as Save As does, starting in the folder of the document its pages came from; saved, it is that file and a document like any other. Closing its tab, its window or the app asks first ("Pages from report.pdf has not been saved yet. Do you want to save it?": Save, Don't save or Cancel). Until it is saved it is not among the recent files, has no remembered view, is not watched and has nothing to reload; undoing every change leaves it unsaved. Its pages are in a working copy in the app's local data folder (`unsaved/`, beside the recovery copies), which goes when the document is saved or closed, and at the next start if Lectrix did not end normally. Crash recovery covers it as any document with unsaved changes, and restores it as a document without a file.

**Atomic save.**

1. Write the new file to a temporary file in the same folder as the target.
2. Flush it to disk.
3. Replace the original in one rename on the same volume.
4. On failure, delete the temp file and leave the original untouched.

A failed save also leaves the document in memory as it was, so saving again (or Save As) is safe: an incremental save is written without moving the document on to the update it wrote, and "Save As (optimized)" optimizes a copy of the document (ADR 0003, "A save that fails").

Documents are opened through a file stream that allows that rename (ADR 0003). On Windows the rename is retried for up to a second while a file is locked, since antivirus scanners and the search indexer open a file just written for a moment. If the target is still locked by another program (Acrobat locks files it has open), show a clear message naming the likely cause and offer Save As.

**Crash recovery.** Every 2 minutes, each dirty document whose content changed since its last copy gets a recovery copy in the app's local data folder (a snapshot with the unsaved changes as an incremental update; damaged files are copied in full). Saving, reloading or closing a document deletes its copy, and so does quitting normally. On the next launch, before opening startup files, Lectrix offers to restore copies a crash left behind: Restore, Discard (asks once more) or Not now (asked again next time; Escape means Not now). A restored document opens in a tab for its own file, marked unsaved, and saves incrementally onto it. Encrypted files ask for their password again.

**External changes.** Watch open files. If a file changes on disk and the document has no unsaved edits, offer to reload; if it has unsaved edits, warn and offer Save As.
