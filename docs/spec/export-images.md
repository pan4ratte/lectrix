# 6.14 Export as images

Part of the Lectrix specification. [AGENTS.md](../../AGENTS.md) holds the rules every change follows and [README.md](README.md) lists these files; sections keep the numbers they share, so "section 5.4" is in AGENTS.md and "section 6.5" in [annotations.md](annotations.md).

A document's pages saved as images: each page drawn as it shows, at a resolution, in colour, greys or black and white, as PNG, JPEG or TIFF, a file a page or (TIFF) one file of them all (ADR 0034). Pages are drawn by MuPDF as the viewer draws them; the document is only read.

- **Where it is:** **File > Export > Pages as images…** (after Print…; a command without a default shortcut, section 6.6), for the document shown, in the viewer or while its pages are organized; there it starts with the selected pages. A page's right-click menu in the viewer has **Export page as image…**, which starts with that page; Organize pages has **Export** on its bar, after Extract, and **Export page as image…** ("Export 3 pages as images…") in its menu, both for the selected pages (section 6.7). Not on the start screen or in the Combine view.
- **Copy page as image** (below, "Copying a page") puts one page on the clipboard instead; the **Snapshot** tool copies or saves part of a page (below).
- What is exported is the document as it is open, its unsaved changes included. A document whose security settings forbid copying its content can be exported all the same (the user's decision, ADR 0034); one that forbids printing too.

## The dialog

Titled "Export report.pdf as images", with "12 pages" under it (", with its unsaved changes" when it has some). The settings on the left, the preview on the right, and **Cancel** (also Esc) and **Export…** (the main button; also Enter) under them.

- **Pages:** **All 12 pages**, **The current page (iv)** ("This page (iv)" from a page's menu), **The 3 selected pages** (from Organize pages), or **Pages** with a field, as the page box and Print take pages (section 6.12): numbers or labels ("i-iv, 1-5, A-1"), an end left out running to the first or last page. A range that names no page says how to type one, under the field, and Export… waits. It starts with what the dialog was opened for.
- **Image:**
  - **Format:** PNG, JPEG, TIFF, with a line on what each is good for under it: "Every pixel kept. Sharp text and drawings; large for photos.", "Small for photos and scans; text and lines get soft edges.", "Every pixel kept, for archives, printers and fax. Large."
  - **Resolution:** 72, 96, 150, 300 or 600 dpi, or **Custom** with a field (10 to 2,400 dpi; another number says so and Export… waits).
  - A page too large for the format at that resolution says so under it, and Export… waits: "Page 3 would be 20,400 × 26,400 pixels, too large for PNG. Choose 1266 dpi or less." A page has at most 150 million pixels (an A4 page at 1,200 dpi has 140), and JPEG at most 65,535 a side.
  - **Colour:** **Colour**, **Greyscale**, and for PNG and TIFF **Black and white**: one bit a pixel, split at Otsu's threshold as Compress splits images (section 6.11). JPEG doesn't keep one bit a pixel, so it doesn't offer it; chosen after it, JPEG exports in greys.
  - **Quality** (JPEG): a slider from 1 to 100 with its value, "Low" and "High" at its ends; 85 to start with.
  - **Compression** (TIFF): LZW or ZIP; black and white is always CCITT Group 4, said in its place. **All pages in one file**, with more than one page: a multi-page TIFF.
  - **Annotations** (on to start with): the pages with their annotations, as the viewer shows them, or without them. Form fields always show. Annotations the annotation list filters out are exported all the same.
- **Files**, with more than one file: **Name by** **Page numbers** or **Page labels** (a segmented control, shown only for a document with page labels), and a line with an example: "A file a page, named after the name you give: report-007.png, and so on."

The dialog's choices are remembered for next time: the format, resolution, colour, quality, TIFF compression and one file, naming and annotations. Not the pages.

## The preview

A page as it will be written: drawn at the resolution, in the colours, with or without annotations, and written in the format, so JPEG's soft edges and a low quality show. **Whole page** fits it in the preview; **Actual pixels** shows one pixel of the file to one of the screen, scrolled. **‹** and **›** go through the pages exported ("3 of 12"); it starts at the page the dialog was opened for, where that is exported. Under it, the page's size in pixels and its file's size: "2,550 × 3,300 pixels · 1.2 MB". It is made again a moment after a setting changes ("Updating…" in its corner meanwhile), and a newer one stops the one being made.

A page of more than 16 million pixels at the resolution chosen is previewed at the highest resolution under that (a Letter page at 600 dpi, at 413), its file's size estimated from that ("about 4.1 MB"), and the preview says "Previewed at 413 dpi, as this page is large."

## Exporting

**Export…** shows the Save dialog ("Export the pages as images", "Export the page as an image", "Export the pages as one TIFF file"), in the document's folder, of the format's file type, with "report.png" for several pages or one TIFF file and "report-007.png" for one page (its number or label, as the files are named).

- **One page**, or **one TIFF file**, is written to the name given. The Save dialog asks before replacing a file of that name.
- **Several pages** are written beside it, each named after it with the page's number, padded to as many digits as the document's last page has ("report-007.png"), or its label ("report-iv.png"; a page without a label has its number). Characters a file name can't have (`< > : " / \ | ? *`) become `_`; a second page with the same label (in any case) has its number added ("report-1 (p. 13).png"). The name given itself is not written.
- An extension the format doesn't have is added: "report" or "report.v2" become "report.png", "report.v2.png"; "scan.jpeg" stays.
- **Files already there:** if some of the files are, the dialog asks first: "3 of these 12 files are already in Pictures: report-001.png, report-002.png and 1 more." ("Both files are…", "All 12 files are…"), with "Replace them with the pages exported now, or keep them and export only the pages whose files aren't there." **Replace** (the main button), **Keep them**, and **Back** (also Esc), to the settings.
- The dialog turns into a progress bar, "Exporting page 3 of 12…", with **Stop** (also Esc). Pages are written into temporary files beside their names, which take their names only once every page is written: stopped, or failing, an export leaves nothing behind and replaces nothing.
- Done, the dialog closes and a notification says "Exported 12 pages of report.pdf as PNG images to Pictures." ("Exported a page of report.pdf to report-007.png in Pictures.", "Exported 12 pages of report.pdf to report.tif in Pictures."), with **Open file location** (Show in Finder on macOS, Show in folder on Linux), which shows the first file in its folder. With every file kept: "Nothing was exported: every file was already in its folder."
- Stopped: "Exporting was stopped. No file was written." A file that can't be written: "Lectrix couldn't write the images in Pictures." with "Check that the drive has room and that you may save files there, then try again, or pick another folder."

## Copying a page

**Copy page as image** puts a page on the clipboard, to paste into a document, a slide, an image editor or a chat. It is in **Edit** (after Copy; a command without a default shortcut, section 6.6), for the current page, or in Organize pages for the page selected (with several, or none, it is disabled: the clipboard holds one image); in a page's right-click menu in the viewer, for the page right-clicked, before Export page as image…; and in Organize pages' right-click menu, before Export page as image…, with one page selected.

- The page is drawn as it is exported (above): as it shows, its annotations and form fields included, unsaved changes too, on white, in colour, at the resolution **Settings > Viewing > Copy page as image** gives (72, 96, 150, 300 or 600 dpi; 150 to start with). The visual adjustments are not applied (section 6.1).
- A page of more than 36 million pixels at that resolution (A4 and Letter fit at 600 dpi) is copied at the highest resolution under it.
- Done, a notification says so: "Copied page iv as an image." (by its label), or "Copied page 3 as an image, at 212 dpi as the page is large." A page too large even at 10 dpi: "This page is too large to copy as an image.", with "Export it with File > Export > Pages as images instead." The clipboard taken by another app: "The page couldn't be copied to the clipboard.", with "Try again. If another app is using the clipboard, close it first."
- On Windows the clipboard gets the page as a PNG and as a bitmap (`CF_DIBV5`), so it pastes into Word, PowerPoint, Paint, browsers and chat apps; on Linux as `image/png`, on X11 and Wayland; on macOS as an image. A document that forbids copying its content can be copied all the same, as it can be exported (ADR 0034).

## The Snapshot tool

**Snapshot**, in the annotation toolbar after Select (a camera; a command without a default shortcut, section 6.6, "Snapshot tool"), frames part of a page to copy or save as an image. It changes nothing in the document, so it works in documents that can't be annotated or changed, and it is not in the Compare view's panes.

- **Framing:** a drag on a page draws the area, kept on that page, dashed in the accent colour, with eight handles. A handle resizes it, a drag inside it moves it (kept on the page), a drag elsewhere frames a new one, and a click elsewhere, or away from the pages, leaves none. It draws over annotations, which it doesn't select. Esc during a drag puts back the area there was; Esc after lets the area go, and a second Esc picks the Select tool (section 8). Picking another tool lets it go too. The cursor is a cross, a four-way arrow inside the area and a resize arrow over a handle, as the handle is on screen.
- **Its bar**, above the area once the drag ends (`SnapshotBar.svelte`): **Copy as image** (Ctrl+C, which copies the area while there is one) and **Save as image…**. The page's right-click menu has **Copy area as image** and **Save area as image…** in place of Copy selection.
- **Copying** draws the area as Copy page as image draws a page (above): at Settings > Viewing > Copy page as image's resolution, at most 36 million pixels, a larger area at the highest resolution under that. "Copied the area as an image." ("…, at 212 dpi as the area is large."). Too large even at 10 dpi: "This area is too large to copy as an image.", with "Save it as an image instead, or select a smaller area."
- **Saving** opens the Export as images dialog for the area: "Save an area of report.pdf as an image", "Page iv" under it; in place of the pages, "Area" and "The area framed on page iv, as it shows on screen."; the preview without ‹ and ›, its first choice **Whole area**; a size too large says "The area would be …"; and **Save…** in place of Export…. The Save dialog is "Save the area as an image", with the name one page gets ("report-iv.png"). Done: "Saved the area as report-iv.png in Pictures.", with Open file location.
- The image is the area as it shows: a page turned on screen (section 6.1) gives an image turned so, and annotations and form fields are on it (Save as image… can leave annotations out); visual adjustments are not applied.

## The files (ADR 0034)

- **PNG:** colour (RGB), grey or one bit a pixel; the resolution (`pHYs`) and sRGB in the file; compressed well (the `png` crate's High).
- **JPEG:** baseline, through libjpeg-turbo as Compress writes it; its colour halved both ways below quality 90; the resolution in its JFIF header.
- **TIFF:** colour and grey with LZW or ZIP and the horizontal predictor, black and white as one strip of CCITT Group 4 (white is zero); the resolution in each page. A file of several pages has a page each.
- Pages are drawn on white, with no transparency.

## Settled details

- The formats, the Save dialog Acrobat's way (one name, the files beside it), and the dialog with a preview: the user's decisions (2026-11-02), as are exporting documents that forbid copying and naming files by number or label.
- WebP and JPEG XL were built and dropped (the user's decision, 2026-11-02): JPEG XL's encoder made the app 2 MB larger, WebP has little demand in a PDF app, and both encoders were AGPL-3.0-only (ADR 0034).
- The defaults: PNG, 150 dpi, colour, quality 85, annotations on, files named by number.
- Copy page as image: through Tauri's clipboard plugin, its resolution a setting, 150 dpi to start with (the user's decisions, 2026-11-02). The page's menus, Edit and Organize pages' menu have it; Organize pages' bar doesn't, as it is full already.
- The Snapshot tool: a tool beside Select, an area with handles, and a small bar with Copy and Save as image…, rather than Acrobat's copying as soon as the area is drawn (the user's decisions, 2026-11-02). Its name and icon, no default shortcut, copying at the Copy page as image resolution, and an image turned as the page shows were chosen without the user (2026-11-03).
