# Known gaps

Part of the status: [docs/status.md](../status.md) is the index a session reads, with one line for each item; this file has the known gaps in full. Keep both current.

- **A book whose cover is a whole jacket in one large image** shows its first page late.
  Measured 2026-10-18 on a 283-page book (OneVision PDFengine): page 1 crops the front out
  of a 1,515 pt wide jacket drawn from one 6035 × 2693 CMYK JPEG with an ICC profile and
  a soft mask that is a second JPEG of that size. The first page is visible after 580 to
  626 ms on battery (the 1,000-page file 100 to 172 ms the same day); the other pages
  render in 11 ms on average. MuPDF decodes only the columns shown, but every row of both
  JPEGs: about 345 ms of the page's render at the 3.5 px per point it is fitted at.
  Its rendered image also leaves the image cache once the pages after it are scrolled
  through, so a jump back to it left it blank for 304 ms on battery (the target is 200).
  Since 2026-10-19 MuPDF decodes JPEG with libjpeg-turbo and copies JPEG rows whole
  (ADR 0024): on mains, the first page shows after 371 to 387 ms instead of 467 to 481,
  and the worst blank after a jump was 196 ms (191 before). What is left is MuPDF's work
  on the decoded image (about 100 ms) and the decode of every row; showing a coarse
  decode first, or decoding only the columns shown, would bring it down further.
- **JPEG 2000 scans.** A page of a book scanned as JPEG 2000 (archive.org style) takes
  about a quarter of a second to render. Measured 2026-10-12 on a 602-page Internet
  Archive book (each page a 1920 × 3032 image with a JBIG2 mask, drawn 1,300 px wide):
  230 ms, of which about 115 ms decodes the JPEG 2000 image, 35 ms its mask and
  background, and 77 ms draws them. Pages are rendered ahead (AGENTS.md section 3), so
  read page by page or scrolled steadily (2,000 px/s) they are there when they come into
  view: 3 of 43 pages came blank, against all 43 before, each for 0.3 to 0.5 s. Scrolled
  fast (6,000 px/s) every page still comes blank, for about 0.26 s, and so does a page
  jumped to, for 0.3 to 0.4 s: one page's render is the wait, and it is not ahead of
  anything there. MuPDF also decodes one JPEG 2000 image at a time, on one thread, under
  a global lock (`fz_opj_lock`). Lifting the lock and giving OpenJPEG threads were both
  tried in a local build of the fork and not adopted, since neither shortens that wait by
  more than a tenth (ADR 0008, "Tried and not adopted"). With such a book open,
  `lectrix.exe` idles at 150 MB (97 MB before pages were rendered ahead: the next page's
  image, and its decoded images in MuPDF's store).
- **Scroll modes** are not remembered per file: a document reopens in the Settings
  choice. Right-to-left documents (`/ViewerPreferences /Direction /R2L`) still put their
  first page on the left in the two-page modes. In single page and two pages, a text
  selection cannot run on into the next row.
- **PDF/A.** Lectrix reads the claim and does not check that a file complies. Nor does it
  write annotations that comply (highlights use transparency, which PDF/A-1 forbids; text
  boxes use Helvetica, which is not embedded), so a save with annotation changes or
  inserted pages removes the claim instead. This errs on the safe side: deleting an
  annotation removes it too, and so does saving a document restored after a crash,
  whatever its changes were. That bookmarks, page labels, page rotation and a full or
  optimized rewrite leave a file compliant follows from reading the standard; no validator
  (veraPDF) runs in the tests. `pdf-cli`'s editing commands work on a document without a
  session and leave the claim alone. Detection and removal are tested with generated files
  that carry only the claim (`pdf-cli gen --pdfa 1b`), not with a real PDF/A file yet (the
  manual checklist has one to make).
- **Annotations the list filters out** are hidden on the page, but not in thumbnails, and
  a filtered-out text box's text can still be selected and found: page text is read from
  the page with all its annotations. Annotations that are not indirect objects (rare; they
  can't be told apart) are hidden on a page only when the filters leave out all of them.
- **Windows.** A tab dragged out of its window shows nothing under the pointer once the
  pointer has left the window (the stand-in is drawn by the window's page). A tab dropped
  on another window goes to the end of its tabs, not to the place it was dropped at. The
  side panes' layout and the thumbnail size are one for all windows: a new window starts
  with what was last stored, and the window changed last decides what the next start
  gets. A search, a text selection and the annotation tool in use stay behind when a tab
  changes windows. Lectrix does not reopen several windows at the
  next start, and has no command for an empty new window. An update downloaded in one
  window and put off with Later is not known to the others: Check for updates there
  downloads it again.
- **Full screen.** The bars and panes slide from the first frames after the key, both
  ways, and the window has its new size about 150 ms after it (measured 2026-10-12 with a
  120 Hz display). The switch itself holds the webview's frames up for 40 to 100 ms,
  with nothing moving meanwhile. Sliding back in, the page's frames show one gap of about
  40 ms mid-slide while the main thread is idle (the compositor's, as the bars and panes,
  just built, are first drawn); whether it shows has not been judged by eye. Full screen
  is not remembered between runs, and each window has its own. The pane
  of a side comes only for a pointer resting at the outermost 4 px of the screen there:
  with another monitor beside that side the pointer passes on to it, and the strip is hard
  to stop in (Alt held, or the panel commands, bring the panes all the same). At the right
  side that strip is part of the pages' scrollbar, 10 px wide: resting on its outer part
  brings the right pane over it. The end-to-end test presses F11 and Alt through
  WebDriver, inside the webview: that a real Alt key opens no system menu, and that
  Alt+F4 and Alt+Tab still do what they do, is in the manual checklist, not yet done.
- **Visual adjustments.** Invert colours keeps hues by CSS's `hue-rotate`, which is an
  approximation: saturated colours come out darker or lighter than they were, and
  photographs look odd, as inverted ones do. Annotations are left out of it and of
  greyscale (their own layer, rendered by Rust): a page with annotations takes a second
  image while either is on, not measured. The layer is drawn after the content, so for a
  moment after a page appears its annotations may be missing. A highlight that lies over
  another annotation is drawn under it on an inverted page; a highlight over a picture
  shows that part of the picture in colour under greyscale. No command (so no shortcut) turns an
  adjustment on or off; the panel does. The start screen's previews and the Combine view
  show pages as they are. Scrolling with adjustments on has not been measured
  (`tests/perf/measure.ps1` runs without them); the filter is applied by the compositor,
  and the pages are rendered and cached as before. Sepia is a layer blended into each page
  (`mix-blend-mode: multiply`), which the compositor does too; not measured either, and
  not looked at on macOS and Linux (WebKit). The sliders in Settings apply when let go, or
  at each key: the pages do not follow a drag there as they do in the viewer's panel.
- **Links.** A link is followed by its `/Rect`: one that covers several lines by
  `/QuadPoints` also answers between them, and is tinted as one block. Links are followed
  with the pointer only (the keyboard does not reach them), so their tooltips need a
  pointer too. Only http and https addresses open, in the system's browser; mail addresses
  and the rest are shown for copying. Bookmarks have no tooltip saying where they lead
  (their inspector says it).
- **Undo history restarts** after each save (ADR 0003) and after restoring a crash
  recovery copy: MuPDF 1.27.2 cannot load a saved journal (ADR 0001).
- **Recovery copies of large damaged files** are full copies and take 1.2 to 1.5 s for
  165 to 173 MB files. That document renders nothing new meanwhile. Copies are written at
  most every 2 minutes, and only after a change.
- **Organizing pages** (AGENTS.md section 6.7, ADR 0018).
  - **After a save that wrote many objects, every edit in a large file is slower**, until
    the file is saved with Save As (optimized), and again each time it is opened. Found
    and measured 2026-10-13 on the 2,597-page book with the most bookmarks of the local
    corpus: with half its pages moved to the end (each of the 1,298 gets another parent
    node, so the save appends 1,310 objects) and saved, a rotation or a move takes 160 to
    180 ms where it took 26 to 36, and a deletion 1.1 s where it took 0.19. Rendering is
    no slower (5 to 13 ms a page either way). The cause is in MuPDF: the cross-reference
    table of such an update has a run for every object or two, and `pdf_get_xref_entry`
    goes through a section's runs one by one for every object it looks up, without using
    the index it keeps of where each object was found. It is neither new nor the
    organizing tool's alone: 1.1.0 does the same, and any save with many scattered changes
    writes such a table (every page rotated, hundreds of annotations repaired; only the
    move was measured). Not fixed. A workaround in Lectrix would make such a section one
    table after loading, which changes the shape MuPDF takes its snapshot fingerprint
    from, and recovery copies are snapshots: it needs the user's go-ahead and tests of
    its own. See "Upstream work" in [docs/upstream/README.md](../upstream/README.md).
  - Edits in a file with thousands of bookmarks (ADR 0018, "Speed in files with thousands
    of bookmarks"). Measured 2026-10-13 on that book (5,023 bookmarks, 4,999 of them by
    name), release build, the session's own re-reading included: rotating a page takes 26
    to 34 ms, moving pages 29 to 36 ms and undoing either 27 to 32 ms, where each took
    0.14 to 0.26 s. What is left is the session reading every page's size, label and
    annotations again after each change, as in any file (13 to 20 ms in the 2,881-page
    dictionary). Still slower there: the first move in a document, which goes through
    every link, bookmark and named destination once (80 to 100 ms); deleting pages, which
    reads the whole outline again and writes the name tree without the names that led to
    the pages (0.19 s); and every bookmark edit, which reads the whole outline again
    (about 40 ms of it, where it was 110).
  - Every edit draws the pictures on screen again, since the document's revision is part
    of what names a page image. Each page keeps its old picture until the new one is
    there. Measured 2026-10-13 with `tests/perf/organize.mjs` (a page moved one place
    with the keys, 36 pictures on screen at the first size): the grid shows the new order
    11 to 35 ms after the key and the pictures are all drawn again after 84 to 113 ms, on
    the generated 1,000-page file, the 2,881-page dictionary and a 1,020-page scan alike;
    15 to 45 ms and 115 to 154 ms on the 173 MB file. On the book with 5,023 bookmarks,
    44 to 62 ms and 136 to 142 ms (96 and 186 ms for the first move), where the grid
    followed after 236 to 256 ms before: there the bookmarks, whose pages have other
    numbers after a move, are sent to the window again.
  - Not yet checked by a person (`docs/manual-checklist.md`, "Organizing pages" and
    "Pages turned on screen"): the start screen's tool, the largest size in windows of
    several sizes, an extracted document through a save, a close and a real crash, and
    the Page labels dialog with Narrator; PDFs dropped from File Explorer on the grid, which the
    test driver cannot do (the gap under the pointer is worked out from the position
    Tauri reports, in device pixels, so it wants trying at more than one display scale);
    a touchpad pinch over the grid; files with pages moved, deleted and extracted opened
    in Acrobat Reader, Edge and Firefox; pages turned on screen in a real scan.
  - **A deleted page stays in the file after an ordinary save.** Saves are incremental
    (AGENTS.md section 5.4), so the original bytes, the deleted page among them, remain
    and can be recovered. Only Save As (optimized) writes a file without it (tested).
    The notification after deleting pages says so and offers Save As (optimized), as the
    user decided on 2026-10-13.
  - Annotations made on a page turned on screen are placed upright as seen there, as on
    a page the document rotates (AGENTS.md section 6.5; decided by the user on
    2026-10-13): a text box carries the page's `/Rotate` plus the turn. What is left: a
    note's icon is drawn with the page, so it lies on its side on a page turned on screen
    only (Acrobat shows it upright); a text box being moved or resized shows a plain
    outline, and its handles are named for the page, not for the box as it is read;
    Acrobat itself has not been tried with such a box (the manual checklist has the
    files, `turned-normal.pdf` and `turned-rot90.pdf`).
  - A text box is fitted to its text by its own `/Rotate` when edited. For another
    app's box whose `/Rotate` is not the page's (none at all on a rotated page, say) that
    is a change: it was fitted as if its text stood upright on the page, which is not how
    MuPDF draws it. No file of the local corpus has such a box.
  - A page turned on screen is rendered once more when a fitted zoom changes with the
    turn (a page on its side is wider); at a zoom that stays, it keeps its pixels. Its
    thumbnail is rendered again for its new width.
  - A turn is kept by the page's place in the file, as the view position is: if another
    program removes or reorders pages before the file is opened again, the turn is on
    whatever page is at that place then.
  - No command, so no shortcut, turns the current page; the Pages panel does. The buttons
    there keep their names ("Rotate current page clockwise"), which do not say that the
    file stays as it is; the changelog and the tab's missing dot do.
  - Inserting pages shows no progress and cannot be stopped (one operation on the
    document, as before); the dialog says "Inserting…" meanwhile.
  - The size of the grid's pictures is not remembered. At the largest size (one page
    filling the view) each picture is a whole page rendered at that size, as the pages
    are while reading below the zoom at which they are drawn in tiles; scrolling the grid
    at that size has not been measured (`tests/perf/organize.mjs` runs at the first
    size).
  - The Page labels dialog does not list the ranges a document has: the grid shows every
    page's label, and a range is taken away with "Continue the numbering of the page
    before". It numbers the pages picked through in page order; a selection made in
    another order is not numbered in that order. What was last picked in it is not
    remembered: it starts as "1, 2, 3 from 1" each time.
  - **Organize pages…** on the start screen shows the grid for a file that opens at once.
    One that asks for its password first opens for reading; the grid is then a click
    away.
  - An extract carries every attached file of the document, whatever pages are picked,
    and nothing of its metadata (title, author, XMP), as a combined file does. While
    pages are extracted, the document renders nothing new.
  - Extracted pages are a document without a file until it is saved (ADR 0019). Its
    pages are written twice: into a working copy in the app's data folder, then where
    the user saves it. Extracted from again, it gives "Pages from Pages from …". After a
    crash it is restored as a document without a file, but its Save dialog no longer
    starts in the folder its pages came from; a crash before its first recovery copy
    (two minutes) loses it. A tab of such a document moved to another window stays one
    (tested in Rust, not by hand). In the Combine view, the working copy could be picked
    as a file from the app's data folder; nothing stops that.
  - A tagged file keeps its structure elements for deleted pages (they point at emptied
    page objects), and article threads and an open action that lead to a deleted page are
    left as they are. Deleting pages therefore removes a PDF/A claim at the next save.
  - A page tree that MuPDF and Lectrix read differently, or one with a page listed twice,
    is refused (`Error::DamagedPageTree`) and not mended. None of the 30 local corpus
    files is refused.
- **Combining** does not carry over the structure tree (the result is not tagged),
  article threads, document-level JavaScript, open actions, viewer preferences or XMP
  metadata. Signatures from signed sources are invalid in the result. Lectrix warns
  before combining or inserting from them.
- **Replies in Acrobat and Foxit** have not been checked by a person yet (ADR 0012): they
  are meant to show in the parent's thread there, which no engine the harness runs can
  confirm. The manual checklist covers it.
- **Repair and PDFgear** (ADR 0017). A user reported that PDFgear stopped showing a
  file's annotations, or showed only some, after Lectrix repaired them. Repair no longer
  rewrites the mirrored corners PDFKit apps write, the one change it made to data such an
  app reads back, but this has not been tried in PDFgear: it is not installed here, and
  the report came from macOS. If PDFgear trips on `/NM` or `/P` instead, the report
  stands. Annotations that keep mirrored corners are drawn from them if later edited in
  Acrobat (an underline then runs along the upper edge); an edit in Lectrix orders them.
- **Marks Lectrix 1.0 made on right-to-left text** fall short by a character at each
  end, and keep mirrored corners. Repair does not move corners, so they stay as they are;
  marking the text again covers all of it.
- **Text marked across both directions** (a Latin word in a Hebrew line) is marked as the
  characters lie on the page, in several quads; selecting such text in the viewer has
  not been checked by a person.
- **Accessibility.** Creating, moving and resizing annotations needs a pointer, and so
  does starting a text selection, which brings up the quick tools (once text is selected,
  Shift and the arrow keys extend and reduce it). So do moving and resizing an
  annotation's comment panel (its place beside the annotation needs neither). A reply's own menu (Edit reply,
  Delete reply) opens only with a right-click; Reply itself is in the row's menu, which
  the keyboard reaches. Editing, deleting and repairing
  work from the keyboard; the bar of a selected annotation is reached with Tab from the
  page, and the controls in its row of the annotation list with Tab from the list. Page text is not exposed to screen readers, and Windows high-contrast themes are
  not handled specifically.
- **Not covered by automated tests:** drag-and-drop from Explorer (an OLE drag can't be
  scripted), touch or stylus input, touchpad pinch zoom, and a tab dragged out of its
  window onto another one (WebDriver's pointer stays inside the page; the end-to-end test
  drops the tab inside its own window, and moves it between windows from its menu). The
  manual checklist has it.
- **Touchscreen pinch** does not zoom: only touchpad pinch and Ctrl+wheel do. The app
  blocks touchscreen pinch (`touch-action`) so it cannot scale the whole window.
- **Test corpus.** `tests/corpus/` is still empty. Geometric cases come from generated
  files (`pdf-core/src/testgen.rs`), and real-world coverage comes from the local corpus,
  which lacks CJK text, signed files, UserUnit pages and annotations made by other apps
  of these types: Squiggly, Circle and FileAttachment.
- Engine differences that no file can change (PDFium ignores `/UserUnit`, and sticky
  notes turn with rotated pages in PDFium and pdf.js) are listed in
  `docs/interop-profile.md`.

- **A change of a document's protection is lost in a crash:** a recovery copy holds the
  document's content, never its passwords (ADR 0026), so a restored document has its
  file's protection and the change is to be made again.
- **Document properties** writes `dc:title` and `dc:description` in the default language
  only, and `dc:creator` as one item: titles another app gave in several languages lose
  the others, and several authors become one ("Ann; Bob"). "Fast web view" reports a
  linearization dictionary at the start of the file even after an incremental save broke
  its linearization (section 6.10, "Settled details").
- **Compress** (section 6.11, ADR 0027):
  - Scans gain little: JBIG2 images are kept, and JBIG2 and MRC for scans come with OCR
    (Leptonica). Group 4 images are written again only from Group 3 or byte-aligned
    rows.
  - Inline images, images in Type 3 glyphs, and images only in an optional-content layer
    hidden by default are not downsampled (their resolution on the page is not found).
  - Images in separations, DeviceN or Lab, with colour-key masks, or with alpha in
    their JPEG 2000 data are left as they were. A JPEG 2000 image that names no colour
    space and is re-encoded is written in a device space, without an ICC profile its
    codestream may carry.
  - Black and white from colour or grey (the Colour setting) is Otsu's one threshold for
    the whole image: uneven lighting in a scan loses detail where an adaptive threshold
    would keep it.
  - A JPEG or JPEG 2000 image under 0.4 bits a pixel is not re-encoded at its own size,
    to save decoding time: a scan in layers (a 2,474-image book) took 137 s, now 20.
  - Not yet compared with Acrobat's Reduced Size PDF and FineReader by a person
    (`docs/manual-checklist.md`, "Compress").
  - Measured on the local corpus (2026-10-19, release build, mains), Lossless: 29 of 30
    files smaller (up to 56%), every sampled page drawn pixel for pixel as before; the
    slowest, 371 byte-aligned Group 4 pages written again, 17 s; a 2,881-page book 4.3 s;
    a 2,597-page book with 100,000 objects 7.7 s.
    Merging duplicates is Lectrix's own (MuPDF's took 150 s on that book).
- **Print** (section 6.12, ADR 0028):
  - On Windows each sheet is an image, at most 600 dpi: sharp on paper, but a job is a
    few megabytes a sheet in the spooler (two text pages in colour through Microsoft Print
    to PDF: 6.5 MB), where Acrobat can send vector drawing. MuPDF has no GDI or XPS
    device.
  - Several pages on a sheet, booklets, posters, and choosing the paper tray by page size
    (Acrobat's) are not offered; trays are the driver's settings (Printer settings…).
  - The preview shows greys with the webview's filter, not the printer's conversion.
  - Tried on Windows with Microsoft Print to PDF (the end-to-end test, and `pdf-cli print`
    with pages turned and in greys). Not yet on a paper printer, nor Printer settings… (the
    driver's own dialog) by a person (`docs/manual-checklist.md`, "Print").
  - Both sides in landscape on Windows: Lectrix swaps `DMDUP_VERTICAL` and
    `DMDUP_HORIZONTAL` for landscape jobs (`windows_print.rs`), reading them as edges of
    the page as it lies; Microsoft's documentation and drivers disagree on whether they
    name the sheet's edges instead, as IPP's do (CUPS gets them unswapped). To be settled
    on a duplex printer: print two landscape pages with "flip on the long edge" and check
    the back is upright when the sheet is turned over its long edge.

## Split view

- **A joined tab moved to another window** (its menu, File > Move tab to new window, a drag) arrives there as one tab per document, not joined: a window gets its tabs from Rust (`list_open_documents`), which knows documents, not views. A second view of a document stays behind and goes. To keep them joined, the handoff would carry the group.
- **The lock is not remembered,** nor is a split: Lectrix does not reopen the tabs of a closed window.
- **Synchronized scrolling counts pages**, not points: documents whose pages differ greatly in size keep their distance page for page, as Acrobat's does.
