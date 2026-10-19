# 0026: Protect writes a password and restrictions, as a tool on a file and as a setting of a document

- Status: accepted (the user's go-ahead, 2026-10-19)
- Date: 2026-10-19

## Context

Unlock (ADR 0025) removes a PDF's protection. Its counterpart gives a file a password to
open it, restrictions on printing, changing and copying, or both. Until now Lectrix only
kept the encryption a file came with (AGENTS.md section 5.4, `PDF_ENCRYPT_KEEP`) and never
chose one.

MuPDF writes encryption on a full save from `pdf_write_options` (`do_encrypt`,
`permissions`, `opwd_utf8`, `upwd_utf8`), which the vendored crate sets
(`PdfWriteOptions::set_encryption` and the others). It cannot add or change encryption
in an incremental update. Reading `pdf_new_encrypt` (`pdf-crypt.c`) shows three things
that shape this decision:

- `PDF_ENCRYPT_AES_256` writes revision 6 (V5, AESV3), as Acrobat X and later do, with a
  random file key, and passwords prepared with SASLprep.
- The permissions are masked to the defined bits and the reserved ones set
  (`(permissions & 0xf3c) | 0xfffff0c0`), so Lectrix only chooses the bits.
- An empty owner password is written as such: under revision 6 an empty password then
  opens the file as its owner, whatever its user password is.

The crate's password setters `assert!` that a password fits MuPDF's 128-byte buffers, so a
longer one would panic.

## Decision

1. **Both a tool and a document setting** (the user's decision). **Protect…** on the start
   screen and in the File menu protects a file on disk at once, in place or as a copy, with
   the same questions as Unlock. With a document shown, the same dialog sets the
   protection the document will be saved with, from the menu or from the new Properties
   dialog (section 6.10).
2. **AES-256 only**, metadata encrypted. No choice of method is offered.
3. **Acrobat's choices for restrictions**: printing (none, low, high resolution), changes
   (five levels) and copying, mapped to the bits as Acrobat maps them (the table in
   `docs/spec/protect.md`). Bit 10 is always set.
4. **The permissions password is required with restrictions** (the user's decision), and
   must differ from the password to open. A file with only a password to open gets a
   random 32-byte permissions password, so an empty one never bypasses it.
5. **Passwords are checked in Rust** before they reach the crate: at most 127 bytes in
   UTF-8 (what revision 6 uses) and no NUL, refused as a plain error, never a panic.
6. **The written file is verified before it replaces anything**, in the hook
   `save_atomic_checked` already has: reopened with each password, and its permissions
   compared with those asked for.
7. **A document's protection change is not a journal step.** Encryption is a write option,
   not an object of the document, so MuPDF's journal cannot hold it. The session keeps
   the pending protection beside the revision; it makes the document unsaved, forces a
   full save, and the dialog changes it back. After the save the session reopens the file
   with the permissions password (or the password to open, if that is the only one), so
   its author is not refused what the file now restricts.
8. **Passwords never reach the disk.** The tool keeps them under a token as Unlock does
   (`src-tauri/src/protect.rs`, sharing `Waiting` with `unprotect.rs`) and forgets them
   once the file is written or Cancel is chosen. A recovery copy does not hold a pending
   protection change, and `ProtectionSettings` leaves them out of its `Debug` output, so
   they reach no log.
9. **PDF/A**: encryption is forbidden there, so a save that adds protection removes the
   claim (section 5.4's rule for changes Lectrix cannot vouch for).

## Alternatives considered

- **Only a tool on a file**, as Unlock is (recommended at first). Not chosen: the user also
  wants protection to be part of a document being edited.
- **Only a document setting**, as Acrobat has it. Not chosen: protecting a file should not
  need it opened and edited first.
- **Protection change as an undo step.** It would need a step in MuPDF's journal that
  changes no object, or Lectrix's own history beside the journal. Not chosen: the dialog
  undoes it plainly, and the history stays MuPDF's.
- **A choice of encryption method** (AES-128, RC4) as Acrobat offers. Not chosen: every
  mainstream reader opens AES-256, and RC4 is broken.
- **An optional permissions password**, filled with a random one when left empty. Not
  chosen (the user's decision): nobody could then lift the restrictions in Acrobat, not
  even the file's author.
- **Keeping the pending protection's passwords in the recovery copy's slot.** Not chosen:
  passwords would be written to disk in the clear.

## Consequences

- Section 5.4 gains a third case: encryption is kept on save, removed by Unlock, and set by
  Protect. AGENTS.md sections 1, 5.4 and 6 change in the commit that builds it.
- The atomic save takes what becomes of the encryption (`save::Encrypt`: keep, remove or
  set; `save_atomic_encrypted`), beside the kind of save, so `SaveKind` stays as it was.
- Reading a document's own restrictions (not the ones that apply once the permissions
  password opened it) goes through the shim (`ffi::crypt_info`). Telling whether a file
  needs a password to open authenticates with the empty one, so the document is
  authenticated with its own password again afterwards (`protection::read`).
- The manual checklist gains a Protect section: each restriction as Acrobat's Document
  Properties > Security shows it, a password to open asked for in Acrobat, Edge, Firefox
  and Foxit, and a non-ASCII password.
