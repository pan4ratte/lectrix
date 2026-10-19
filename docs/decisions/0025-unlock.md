# 0025: Unlock removes a file's password and restrictions with any password that opens it

- Status: accepted (decided by the user, 2026-10-19)
- Date: 2026-10-19

## Context

The first of the tools planned after v1 (`docs/status.md`, "Planned tools") is Unlock: a
tool on the start screen and in the File menu that removes a PDF's protection. Until now,
AGENTS.md section 5.4 said encrypted documents keep their encryption on save and that
their permission flags are respected: Lectrix asks for the password, refuses edits the
document forbids, and writes the encryption back.

A PDF can carry two passwords. The open (user) password is needed to read it. The
permissions (owner) password guards the restrictions: printing, copying, changing,
annotating, assembling. A file can have restrictions without an open password: it opens
in any reader, which is trusted to honour the restrictions. MuPDF reads either kind once a
password that fits is given (`pdf_authenticate_password` says which one it was), and
writes a file without encryption on request (`PDF_ENCRYPT_NONE`).

## Decision

1. **Whatever password opens the file is enough**, the restrictions included, and a file
   with restrictions alone needs no password. This is the user's decision, against the
   alternative recommended to them (below).
2. **The user chooses where it goes**, every time, in a dialog once the file is open:
   **Save** (the file itself, replaced in one rename), **Save as a copy** (a file the Save
   dialog names, starting in the original's folder with "report (unlocked).pdf"; the
   original is only read) or **Cancel**. Also the user's decision. Either way, what was
   saved opens in a tab.
3. **The file is written whole**, keeping every object (`SaveKind::Decrypted`: a full
   save with `PDF_ENCRYPT_NONE`). An incremental update cannot take encryption away.
   Nothing else changes: no garbage collection, no recompression.
4. **Signed files** are warned about in that dialog. Writing the file whole means the
   signed bytes are no longer the file's, so its signatures will not be valid in what is
   saved.
5. **The password goes no further than the steps.** Rust keeps the picked file and its
   password under a token between them (`src-tauri/src/unprotect.rs`), so the webview
   never names a path, and forgets both once the file is written or Cancel is chosen. A
   copy's Save dialog given up keeps them, and the dialog is asked again. The token comes
   from the counter the documents' password prompts use, so the two never share one.

Section 5.4 keeps its rule for documents open in tabs: they keep their encryption and their
restrictions. Only Unlock writes a file without them.

## Alternatives considered

- **Acrobat's rule** (recommended to the user): the open password is enough only when the
  file forbids nothing; otherwise the permissions password is needed, so the tool cannot
  be used to get around restrictions the author set. Not chosen: the user wants Unlock to
  remove the restrictions of files they can open.
- **Always the permissions password.** Stricter than Acrobat, and of no use for a file
  whose only password is the open one. Not chosen.
- **Always a new file** (as first built: the copy's Save dialog straight after the
  password). Not chosen: the user wants the file itself unlocked as readily as a copy.
- **Opening the result as a document without a file (ADR 0019)**, saved later. Not
  chosen: the point of the tool is the file on disk.

## Consequences

- Section 5.4's "respect permission flags" now covers documents being edited, and Unlock
  is the stated exception. Its spec is `docs/spec/unlock.md` (section 6.8).
- A file whose only protection is restrictions loses them without a password, as
  `qpdf --decrypt` does. The restrictions of such a file are only ever as strong as the
  reader honouring them; Lectrix no longer honours them in this one tool.
- Files that MuPDF cannot open, such as those protected with a certificate rather than a
  password, cannot be unlocked: the error says the file can't be opened, as opening it
  does.
