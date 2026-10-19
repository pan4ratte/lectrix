# 6.10 Document properties

Part of the Lectrix specification. [AGENTS.md](../../AGENTS.md) holds the rules every change follows and [README.md](README.md) lists these files; sections keep the numbers they share, so "section 5.4" is in AGENTS.md and "section 6.5" in [annotations.md](annotations.md).

What a document is: its file, its description, its protection and its fonts, in one dialog.

- **Where it is:** **Document > Properties…** (Ctrl+D, as in Acrobat), at the end of the menu, for the document shown. A protected document's tab lock opens it at Security, and its tooltip says so ("Protected: click to see its protection").
- **The dialog**, titled "Properties", has its groups as tabs with icons down the left side, as Settings has (section 6.6): **Description**, **Security** and **Fonts**, with **OK** (the main button) and **Cancel** (also Esc) under the tab shown. It opens at Description, or at Security from the lock. OK changes the description if it was changed; Cancel forgets what was typed.

## Description

- **File:** the document's name at the head of the tab, then the folder its file is in, its size ("2.4 MB (2,503,112 bytes)"), and the PDF version it declares (the higher of the header's and the catalog's `/Version`). A document that is no file yet (section 7) shows "Not saved yet" in place of the folder and size.
- **Title, Author, Subject, Keywords:** text fields, as the document has them. Changed and confirmed with OK, they are one undo step ("Change properties"), saved with the document's next save.
  - They are written with their ends trimmed to the document information dictionary (created if the document has none; an empty field takes its entry out) and, if the file has XMP metadata, to its matching properties (`dc:title` and `dc:description` in the default language, `dc:creator` as one item, `pdf:Keywords`) and its `xmp:MetadataDate`, so that readers which prefer the XMP (Acrobat among them) show the same. A property the XMP lacks is added in a description of its own; the rest of the XMP stays as it was. XMP that can't be read as XMP is left alone.
  - Text is written as section 6.2 writes bookmark titles: plain ASCII as it is, anything else as UTF-16BE with a byte-order mark.
  - Where the document's security settings don't allow changes, the fields can't be edited, and a line says so with an **Unlock…** button (section 6.8), which closes the dialog and unlocks the document. A file opened for reading only as PDF/A (section 5.4) has them read-only until Enable editing, and a line says so.
  - Changing them keeps a PDF/A claim: the information dictionary and the XMP are written to match, as PDF/A requires.
- **Read only**, under "About the document": Created and Modified (the document's dates from the information dictionary, else from the XMP, in the system's date and time format), Application (`/Creator`), PDF producer (`/Producer`), Pages, Page size of the page shown (in millimetres, or inches where the system's language is US English, with the paper's name when it is a standard one: "210 × 297 mm (A4)"), Tagged PDF (Yes or No), Fast web view (Yes for a linearized file, or No), PDF/A (the level claimed, or None; a claim, not a check), and Signatures (how many fields hold one, or None).
- A value the document doesn't have shows a dash.

## Security

- **Protection:** None, Password to open, Restrictions, or Password to open and restrictions, with the encryption method (AES 256-bit, AES 128-bit, RC4 128-bit, RC4 40-bit).
- **What it allows**, for a file that restricts something: Printing (High resolution, Low resolution or Not allowed), Changes (as the Protect dialog names them, section 6.9), Copying text and images, Commenting, Filling in forms, Inserting, deleting and rotating pages, each Allowed or Not allowed, as the file sets them. If the document was opened with its permissions password, a line says that nothing is restricted while it is open in Lectrix. A file with a password to open and no restrictions says "It restricts nothing once it is open."
- **Change protection…** opens the Protect dialog for the document over the properties (section 6.9), disabled while the document is open for reading only. A change made and not saved yet is shown under the protection, as **When saved** ("Password to open and restrictions", or "None").

## Fonts

- Every font the document uses, gathered when the tab is first shown (a line says "Looking for fonts… page 120 of 800" meanwhile; closing the dialog stops it), in the order of the pages they first appear on, each once.
- For each, in a table: its name (without a subset's prefix), its type (Type 1, Type 1 (CFF), TrueType, OpenType, Type 3, Type 0 (CID, CFF), Type 0 (CID, TrueType)), whether it is embedded (Embedded, Embedded subset, Not embedded, or "Not embedded (standard font)" for the 14 fonts every reader has), and its encoding (Windows (ANSI), Mac Roman, Identity-H (Unicode)…, Custom, or Built-in).
- Fonts are looked for in the pages' resources, the forms they draw and their annotations' appearances, 25 pages at a time.
- A font that is not embedded, other than the 14 standard ones, is marked (a warning sign, "Readers draw this text with a font of their own"), and a line under the table says why it matters: readers draw such text with a font of their own, so it can look different, or show the wrong characters, from one reader to another. A document without fonts says so.

## Settled details

- A Properties dialog holding the description, security and fonts (the user's decision, 2026-10-19).
- The description is editable; the dates, the application and the producer are not.
- Changing the description keeps a PDF/A claim, since the information dictionary and the XMP are written to match.
- "Fast web view" says only that the file starts with a linearization dictionary; a file saved incrementally since keeps saying Yes, though its linearization no longer holds.
