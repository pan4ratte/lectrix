# 6.9 Protect

Part of the Lectrix specification. [AGENTS.md](../../AGENTS.md) holds the rules every change follows and [README.md](README.md) lists these files; sections keep the numbers they share, so "section 5.4" is in AGENTS.md and "section 6.5" in [annotations.md](annotations.md).

A PDF given a password to open it, restrictions on printing, changing and copying it, or both (ADR 0026). It is the counterpart of Unlock (section 6.8), and works the same two ways: as a tool on a file, which writes the file or a copy of it at once, and as a setting of a document open in a tab, which its next save writes.

- **Where it is:** **Protect…** on the start screen, after Unlock…, with a closed lock for its icon (section 6.1), and **File > Protect…**, after Unlock…, at any time (a command without a default shortcut, section 6.6). With a document shown, the command changes that document's protection; otherwise, and from the start screen, it asks for one PDF in the Open dialog ("Protect a PDF") and protects that file.
- **The dialog**, the same for both, titled "Protect report.pdf":
  - **Require a password to open the file**, a checkbox; under it **Password** and **Confirm password**.
  - **Restrict printing, changes and copying**, a checkbox; under it **Permissions password** and **Confirm password**, then **Printing allowed** (a dropdown: Not allowed, Low resolution (150 dpi), High resolution), **Changes allowed** (a dropdown: None; Inserting, deleting and rotating pages; Filling in forms and signing; Commenting, filling in forms and signing; Any except extracting pages) and **Allow copying of text, images and other content** (a checkbox).
  - The fields under a checkbox are disabled while it is clear. Each password field has a button that shows or hides its text (an eye, "Show password").
  - **Protect** (the main button) and **Cancel** (also Esc). Protect is disabled until what is ticked is complete. A field that would keep it disabled says why under itself: "The passwords don't match.", "Use a different password from the one to open the file.", "This password is too long." The permissions password is required whenever restrictions are ticked, as in Acrobat.
  - New, both checkboxes are clear, printing is High resolution, changes None and copying allowed. For a file or document that is protected already, the checkboxes and the restrictions are as it has them now; its passwords can't be read back, so they are typed again. What the dialog sets replaces the protection the file had.
- **The tool** (from the start screen, or the menu with no document shown). A protected file asks for its password first, as Unlock does ("report.pdf is protected. Enter its password to change its protection.", the button **Continue**); any password that opens it will do (ADR 0025). Then the dialog, then a question, as Unlock asks it: "Protect report.pdf? Save it with this protection, or save a protected copy and keep the original as it is." **Save** (the main button), **Save as a copy** or **Cancel** (also Esc), which writes nothing.
  - **Save** writes the file itself, whole, in place (the atomic save of section 7), and opens it in a tab: "report.pdf is now protected." If it is open in a tab already without unsaved changes, that tab reads it again at once, where it was, without asking for the new password; a tab with unsaved changes is left as it is, and warns that its file changed on disk (section 7).
  - **Save as a copy** shows the Save dialog ("Save a protected copy"), in the original's folder with "report (protected).pdf". The original is only read; the copy opens in a tab: "Saved report (protected).pdf with its protection. The original file is unchanged." A Save dialog given up asks the question again. A copy saved over the original is the same as Save.
  - The tool needs at least one checkbox ticked; removing protection from a file is Unlock (section 6.8).
- **A document's protection** (the menu with a document shown). The dialog's title is the same; Protect sets the protection the document will be saved with, and nothing is written yet. The document is then unsaved, and its next Save, Save as or Save as (optimized) writes it. Clearing both checkboxes of a protected document removes its protection at the next save, and the main button then reads **Remove protection**.
  - Changing protection is not a step of the undo history: it is how the file will be written, not what it holds. The dialog changes it back, and Reload from disk forgets it.
  - The tab's lock follows the protection the document will be saved with, from the moment Protect is pressed.
  - The save that writes it rewrites the whole file, since protection can't be added or changed by appending to a file (section 5.4). Once it is written, the document goes on with its new protection, opened with the permissions password if it has one and with the password to open it otherwise, so nothing it now restricts is refused to the person who set it.
  - Passwords are never written to disk, so a recovery copy (section 7) doesn't hold a protection change: a document restored after a crash has its file's protection, and the change is to be made again.
  - A document that is no file yet (section 7) can be protected; its first save writes the protection.
  - A file opened for reading only as PDF/A (section 5.4) refuses this as any change, until Enable editing.
- **Signed files.** Rewriting the whole file means its signatures will not be valid in what is saved, as with Unlock. The question (the tool) or the dialog (a document) says so: "This file is signed. Its signatures will not be valid once it is saved with new protection, because the whole file is rewritten."
- **PDF/A.** PDF/A does not allow encryption, so a file saved with a password or restrictions no longer claims PDF/A, and says so in the notification that a save removed the claim (section 5.4), which then gives this as the reason ("PDF/A doesn’t allow a password or restrictions, so Lectrix removed the claim when saving the file with them."). Removing protection keeps the claim.
- **A file that can't be opened** (damaged, not a PDF, protected with a certificate) says so in an error notification, as opening it does.

## How it is written

- AES with 256-bit keys (security handler revision 6, as Acrobat X and later write), metadata encrypted too. There is no choice of method: RC4 is broken, and the 128-bit AES of Acrobat 7 is no longer needed by any mainstream reader.
- The restrictions are the permission bits as Acrobat sets them for the same choices:

  | Choice | Bits allowed |
  | --- | --- |
  | Printing: Not allowed | none |
  | Printing: Low resolution | 3 (print) |
  | Printing: High resolution | 3 and 12 (print, high quality) |
  | Changes: None | none |
  | Changes: Inserting, deleting and rotating pages | 11 (assemble) |
  | Changes: Filling in forms and signing | 9 (fill forms) |
  | Changes: Commenting, filling in forms and signing | 6 and 9 (annotate, fill forms) |
  | Changes: Any except extracting pages | 4, 6, 9 and 11 (modify, annotate, fill forms, assemble) |
  | Allow copying | 5 (copy) |

  Bit 10 (text for screen readers) is always allowed, as PDF 2.0 says it is.
- A file with a password to open but no restrictions gets a random permissions password, 32 bytes nobody is shown, and every permission. A permissions password left empty would let an empty password open the file as its owner, around the password to open it.
- Passwords are Unicode, prepared as revision 6 requires (SASLprep, which MuPDF does), up to 127 bytes in UTF-8 each.
- Before the new file replaces anything, Lectrix opens it again and checks that the password to open it opens it, that the permissions password lifts every restriction, and that the restrictions are the ones chosen. A file that fails is deleted and the original stays as it was.

## Settled details

- Both a tool on a file and a setting of a document (the user's decision, 2026-10-19).
- Restrictions need a permissions password typed by the user, as in Acrobat (the user's decision, 2026-10-19). The dialog does not say that some apps ignore restrictions.
- Protecting a protected file replaces its protection; its restrictions are filled in, its passwords typed again (the user's decision, 2026-10-19).
- A protection change is not an undo step, and a recovery copy does not keep it.
- The file is written whole, keeping every object: nothing is optimized or recompressed (except by Save as (optimized), when that is how a document's change is saved).
