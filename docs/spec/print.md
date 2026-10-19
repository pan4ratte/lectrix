# 6.12 Print

Part of the Lectrix specification. [AGENTS.md](../../AGENTS.md) holds the rules every change follows and [README.md](README.md) lists these files; sections keep the numbers they share, so "section 5.4" is in AGENTS.md and "section 6.5" in [annotations.md](annotations.md).

A document printed on one of the system's printers, from a dialog of Lectrix's own with a preview of each sheet (ADR 0028).

- **Where it is:** **File > Print…** (Ctrl+P; ⌘P on macOS), for the document shown, in the viewer or while its pages are organized. Not on the start screen or in the Combine view. What prints is the document as it is open, its unsaved changes included.
- **A document that forbids printing** (its security settings, section 5.4) does not open the dialog; a notification says "report.pdf doesn’t allow printing." with "Its security settings forbid it. Ask its author for a copy that can be printed." One that allows printing at low resolution only prints at 150 dpi, as Acrobat does, and the dialog says so.

## The dialog

Titled "Print report.pdf", with "12 pages" under it (", with its unsaved changes" when it has some). The settings on the left, the preview on the right, and **Cancel** (also Esc) and **Print** (the main button; also Enter) under them.

- **Printer:** a dropdown of the system's printers, the default one marked "(default)"; a network queue (`\\server\queue`) is named "queue on server". It starts with the printer used last, if it is still there, else the system's default. While they are listed, "Looking for printers…"; with none, a line says to add one in the system's settings, and Print is off.
  - **Printer settings…** (Windows) opens the printer driver's own dialog, for what only the driver knows: paper trays, print quality, stapling. What it sets holds for the jobs that follow on that printer while Lectrix runs, and the dialog's paper and sides follow it.
- **Copies** (1 to 999), and **Collate** once there is more than one. The printer makes the copies when its driver can; otherwise Lectrix sends each copy, with a blank back between copies printed on both sides where one would start on the back of the last.
- **Pages:**
  - **All 12 pages**, **The current page (iv)**, or **Pages** with a field, as the page box takes pages (section 6.1): numbers or labels ("i-iv, 1-5, 8", "A-1-A-5"), printed in the order typed. A range with an end left out runs to the first or the last page ("5-"); one that runs backwards prints backwards. A range that names no page says how to type one, under the field, and Print waits.
  - Under them, which of these pages: **All of them**, **Odd pages only** or **Even pages only**, counted among the pages chosen (the fronts and the backs, for printing on both sides by hand), and **Last page first**.
- **Layout:**
  - **Size:** **Fit to the paper** (larger or smaller, to fill the printable area, centred), **Actual size** (centred on the paper, and moved inside the printable area where it fits there), **Shrink pages too large for the paper**, or **Custom scale** with a field (10 to 1000 %).
  - **Orientation:** **As the pages are**: a job of landscape pages prints on landscape sheets; in a job that mixes them the sheets are portrait, and a landscape page is turned on its sheet, back the way its `/Rotate` turned it (so a page turned for reading on screen prints upright), else to the left. Or **Portrait** or **Landscape**, which turn no page.
  - **Paper:** the printer's paper sizes, starting with the one it is set to.
  - **Sides**, for a printer that prints on both: **One side**, **Both sides, flip on the long edge**, **Both sides, flip on the short edge**.
- **Print:** **Annotations** (on to start with; off, the pages print without them; form fields always print) and **In greys (black and white)**. Annotations print as Acrobat prints them: only those with the Print flag, and content a file marks as not for printing is left out.

## The preview

The sheet as it prints, as large as fits: the paper, the page placed on it as the settings place it, cut at the printer's margins, drawn as it prints (the Print flags, with or without annotations, in greys). **‹** and **›** go through the sheets of one copy ("3 of 12"), and the paper is named under them ("A4, 210 × 297 mm", or inches where the system's language is US English). It starts at the page shown, where that prints.

## Printing

Print turns the dialog into a progress bar, "Sending page 3 of 12 to the printer…", with **Stop** (also Esc), which stops sending pages; those sent may still print. Once the printer has the job, the dialog closes and a notification says "Sent 12 pages of report.pdf to Office Laser." A printer that can't be reached, or refuses the job, is named in an error notification ("Lectrix couldn’t print on Office Laser." with "Check that the printer is on and connected, then try again, or pick another printer.").

The dialog's choices are remembered in the app's data for next time: the printer, size and scale, orientation, sides, annotations, greys and collate. Not the pages or the copies.

## How it prints (ADR 0028)

- **Windows:** each page is drawn by MuPDF as it prints at the printer's resolution, at most 600 dpi (150 for a document that allows low resolution only), in bands of 16 MB, and sent through the print spooler (GDI). The paper, orientation, sides, colour, copies and collation are the driver's settings for the job.
- **Linux and macOS:** the sheets are written as a PDF, each a page the size of the paper with the document's page placed on it (vector, through MuPDF's PDF writer), and handed to CUPS (`lp`) with the paper, sides, copies, collation and greys as its options. CUPS does not say where a printer's margins are, so pages are fitted inside margins of 12 pt there. There is no Printer settings… button.

## Settled details

- Lectrix's own dialog with a preview, not the system's (the user's decision, 2026-10-19): the system's dialog on Windows has no preview, and no dialog of the system's knows about sizing, page labels or annotations.
- The defaults: Fit to the paper, As the pages are, Annotations on, one side, collated.
