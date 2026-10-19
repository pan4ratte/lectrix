# 0028: Print has a dialog of Lectrix's own, and draws pages for the printer itself

- Status: accepted (the dialog decided by the user, 2026-10-19)
- Date: 2026-10-19

## Context

Printing was not part of v1 (AGENTS.md section 1). Users ask for it, from the File menu and
with Ctrl+P, and the quality bar is Acrobat's: its Print dialog previews each sheet, sizes
pages (fit, actual size, shrink oversized, a custom scale), turns them to the paper, prints
by page label, leaves annotations out on request, and prints in greys.

Two questions: which dialog, and how pages reach the printer.

**The dialog.** On Windows a desktop app can show the classic Print dialog (`PrintDlgEx`):
the printer, a range of page numbers, copies, collation, and no preview. The Windows 11
dialog with a preview is open only to apps that serve every page through WinRT's
`IPrintDocumentPageSource` and Direct2D's print control, and offers nothing a PDF needs
either. WebView2's own printing prints the webview, which would need every page rendered
into the page first. Linux's GTK dialog and macOS's print panel differ again.

**The pages.** MuPDF has no GDI device, so a page cannot be played onto a Windows printer
as vector drawing. Handing the file to the system's "print" verb prints with whatever app
owns PDFs, not Lectrix, and without its unsaved changes. Some printers take PDF directly,
most through Windows' drivers do not. CUPS, on Linux and macOS, takes PDF from any app and
renders it for the printer itself.

## Decision

1. **A dialog of Lectrix's own** (the user's decision), with the choices Acrobat offers
   that matter most and a preview of each sheet (section 6.12). What only the driver knows
   (trays, quality, finishing) stays in the driver's dialog, which **Printer settings…**
   opens (`DocumentPropertiesW` with `DM_IN_PROMPT`); its settings are kept for the jobs
   that follow while Lectrix runs.
2. **Pages print as MuPDF prints them**: content, annotations and form fields run with the
   "Print" usage (`lectrix_pdf_run_page_for_print` in `shim.c`), which leaves out
   annotations without the Print flag and optional content hidden for printing, as Acrobat
   does. The preview draws the same display list (`layer=print` in the page protocol).
3. **Placement is Lectrix's** (`pdf_core::print::place`): fitted to the printable area the
   printer reports, or at actual size centred on the paper, a page turned a quarter where
   its orientation differs from the sheet's, back the way its `/Rotate` turned it.
4. **Windows: bands of pixels through GDI** (`platform/windows_print.rs`). A job is a
   printer device context made from the driver's settings with the job's paper,
   orientation, sides, colour and copies merged in; each page is drawn by MuPDF at the
   printer's resolution, at most 600 dpi, in bands of 16 MB, and sent with
   `StretchDIBits` (24-bit, or 8-bit grey). 600 dpi keeps text as sharp as laser printers
   make it without a 130 MB image a sheet at 1200 dpi. Copies are the driver's when it can
   make them all; otherwise Lectrix sends them, with a blank back between copies printed
   on both sides where needed.
5. **Linux and macOS: a PDF of the sheets through CUPS** (`platform/cups.rs`): written with
   MuPDF's PDF writer, so the pages stay vector, and handed to `lp` with the paper, sides,
   copies, collation and greys as options. `lpstat` lists the printers and `lpoptions -l`
   what each takes. CUPS does not report the printable area, so pages are fitted inside
   12 pt margins there.
6. **Permissions** are kept: no printing where the file forbids it, 150 dpi where it allows
   low resolution only.
7. **No new dependency.** The Windows calls come from the `windows` crate already pinned
   for DirectWrite, with more of its features (`Win32_Graphics_Gdi`,
   `Win32_Graphics_Printing`, `Win32_Storage_Xps`, `Win32_Foundation`); CUPS's tools are
   run as commands.

## Alternatives considered

- **The system's dialog.** Less work, but no preview on Windows, and no way to offer
  sizing, page labels, annotations or greys; Lectrix would have to choose them silently.
- **The Windows 11 print dialog** through WinRT and Direct2D. A preview, but the system's,
  without what a PDF needs, and a large body of COM code for one platform.
- **XPS through the Print Document Package API.** Vector output on Windows, but MuPDF has
  no XPS writer device, so pages would still be drawn as images or need one written.
- **Printing a PDF on Windows** to printers that take it. Few drivers do, and telling them
  apart is not reliable.
- **Rasterizing for CUPS too.** Simpler to keep alike, but large spool files and softer
  text, where CUPS renders vector PDF itself.

## Consequences

- On Windows a sheet is an image: an A4 sheet at 600 dpi in colour is a few megabytes in
  the spooler (two text pages through Microsoft Print to PDF: 6.5 MB). Fine on a printer,
  larger than a vector job.
- The CUPS path has been built and its parsers tested, but not yet tried on a Linux or Mac
  printer (`docs/status.md`).
- `pdf-cli printers` and `pdf-cli print` print headlessly, for tests and debugging;
  `LECTRIX_PRINT_OUTPUT` has the app write each job to a file (Microsoft Print to PDF then
  asks for no name), which the end-to-end test uses.
