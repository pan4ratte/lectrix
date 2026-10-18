# Status

**v1 is complete (2026-10-04).** All seven build phases were finished, reviewed by the
user and passed CI. This file lists what v1 measures and what it does not do yet. Keep it
current when a gap closes or a new one is found.

The detailed phase reports (what each phase built, its measurements, the decisions asked
for and the user's answers) were kept in `docs/progress.md` until v1 was complete. They
remain in git history: `git show 565fdb1:docs/progress.md`. ADRs that cite
`docs/progress.md` refer to that file.

## What v1 contains

Everything in AGENTS.md sections 1 and 6: the viewer with tabs, bookmarks, page labels,
combining files and inserting pages, the seven annotation types with an annotation list,
and repair. Also: undo and redo for every edit, atomic and incremental saving, crash
recovery, recent files with each file's last view, Settings (author name; System, Light
or Dark appearance), an About dialog with a button to the AGPL source on GitHub, and an installer
that can register Lectrix for PDF files (ADR 0007) and installs for the user or for
everyone (ADR 0015). Up to 1.1.0 there was an MSI as well (ADR 0020).

Since v1: releases on GitHub for Windows, Linux and macOS, built by the Release
workflow with attested installers, and an in-app updater with a Settings switch
(ADR 0011). Replies to annotations, written in the annotation list (ADR 0012). Scroll
modes (single page, two pages, each continuous or not), with a Settings default. A
changelog (`CHANGELOG.md`), shown by Help > What's new, before and after an update, and
used as each release's description. Update checks once a day. Files that claim PDF/A
open for reading only until Enable editing, with a Settings switch, and lose the claim
when a save may have broken it. Several windows: a tab moves into a window of its own or
into another window, with its document as it is. Full screen (F11): the pages alone, with
the toolbar floating and the side panes brought by the pointer or by Alt held. Visual
adjustments (brightness, contrast, sepia, inverted colours, greyscale) for a document's pages,
on screen only and remembered per file. The pages next to those on screen are rendered
ahead, so that pages slow to render (scans) are there when scrolled to. Organizing pages
(AGENTS.md section 6.7, ADR 0018): a document's pages as a grid in the viewer's place,
where they are moved, rotated, deleted, inserted from other files and extracted into a
new document, which is not a file until it is saved (ADR 0019), and where the page labels
are set in a dialog, in place of their side panel. The start screen has it among its
tools. Rotating in the viewer turns a page on screen only, remembered per file.

## Performance

Last measured 2026-10-13, with the page organizing tool in: release build, PNG page
images, a mid-range laptop on battery (Balanced power plan) with its own display,
2560 x 1600 at 120 Hz and 1.5x scaling (page images at 2.28 px per point).

| Target (AGENTS.md section 2) | Measured | Met? |
| --- | --- | --- |
| Window visible < 1 s | 38 to 106 ms; 146 to 224 ms for the first start of a series | Yes |
| First page visible < 1 s | 1,000-page file 96 to 139 ms; 2,881-page dictionary 133 to 136 ms; the 2,597-page book with 5,023 bookmarks 360 to 364 ms; a 33-megapixel JPEG 2000 scan 627 to 639 ms | Yes |
| No blank page > 200 ms while scrolling | 1,000-page file: none at steady speed, worst 22 to 26 ms fast, 104 to 125 ms jumping | Yes, except JPEG 2000 scans (below) |
| `lectrix.exe` < 200 MB idle (ADR 0002) | 58 to 59 MB with the 1,000-page file, 63 to 64 MB with the dictionary, 98 to 99 MB with the 5,023 bookmarks, 115 MB with the JPEG 2000 scan | Yes |
| WebView2 growth rule (ADR 0002) | Whole process tree 525 to 545 MB idle with the 1,000-page file, 530 to 533 MB with a one-page document (the document's size adds nothing), 418 to 446 MB for an empty window. The three rounds of scrolling were not run again | Yes, as far as checked |

Version 1.1.0, installed and measured the same way on the same day, gives the same
numbers (first page of the 1,000-page file 101 to 153 ms, worst blank after a jump
107 ms, the JPEG 2000 scan 606 to 630 ms, the dictionary 136 to 140 ms), so the
organizing tool changed none of them. One differs for the better: the book with 5,023
bookmarks showed its first page after 413 to 417 ms, since its bookmarks' names were
looked up one by one (ADR 0018).

Three numbers are higher than in the table of 2026-10-04. Looked into on 2026-10-13 with
the release build of that evening (`folio.exe`, still in `target/release`), measured
again with that day's script:

- **The 1,000-page file's first page (73 to 80 ms then) and a jump's worst blank (46 to
  54 ms then) moved with the conditions of the run, not with the code.** The old build
  gives today's numbers today: 93 to 134 ms and 103 ms. Today's run was on battery, on
  the Balanced power plan, on the laptop's own display (2560 x 1600 at 120 Hz); what
  the run of 2026-10-04 had was not recorded, beyond 1.5x display scaling. Which
  condition makes the difference was not found out. `measure.ps1` prints the display
  and the power state from now on.
- **The JPEG 2000 scan's first page (376 to 382 ms then, 372 to 406 ms in the old build
  today) takes 606 to 639 ms in 1.1.0 and now, and that is the page being shown once,
  fitted, where it was shown twice.** Until 2026-10-05 (d93e588) the first page was
  requested at 100% before the viewer knew its size, shown, then fitted and rendered
  again, a visible jump on every open; the number was the time to the 100% page. At
  1.5x display scaling 100% is 2.0 px per point, and fit width in the window is about
  2.3 (2.28 for the 1,000-page file's pages).
  This scan's image decodes at a quarter of its resolution up to 2.05 px per point
  and at half from 2.1 on (ADR 0008: the resolution drawn, in halvings), which takes
  230 ms and 455 ms (`pdf-cli bench --scale`). So the page that used to show first was
  the cheaper one, and the fitted page came after another render of about 0.46 s; now
  the fitted page is the first and only one. Nothing was lost, and by these numbers the
  fitted page is there sooner than it was (that was worked out, not timed in the old
  build), but the number is the higher one from now on. It could be
  brought down only by showing the coarser decode first and the finer one after it,
  which is a change to rendering that has not been asked for.

Scripts: `tests/perf/measure.ps1` and `tests/perf/memory-over-time.ps1`; for the grid of
pages, `tests/perf/organize.mjs` (its numbers are under "Organizing pages" below).

## Known gaps

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
  photographs look odd, as inverted ones do. No command (so no shortcut) turns an
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
    its own. See "Upstream work".
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

## Split view

- **A joined tab moved to another window** (its menu, File > Move tab to new window, a drag) arrives there as one tab per document, not joined: a window gets its tabs from Rust (`list_open_documents`), which knows documents, not views. A second view of a document stays behind and goes. To keep them joined, the handoff would carry the group.
- **The lock is not remembered,** nor is a split: Lectrix does not reopen the tabs of a closed window.
- **Synchronized scrolling counts pages**, not points: documents whose pages differ greatly in size keep their distance page for page, as Acrobat's does.

## Platform gaps (macOS and Linux)

CI builds the app and its tests on both, and runs the tests without failing the job.
Nothing has been run interactively there.

- **Fonts:** no index of installed fonts (ADR 0005). Non-embedded fonts other than the
  base 14 render with MuPDF's substitutes, and non-embedded CJK text has no glyphs. This
  needs fontconfig (Linux) and Core Text (macOS) behind the existing `SystemFonts` trait.
- **Window:** the title bar follows each platform (ADR 0021): the system's traffic lights
  over its left end on macOS, GTK's button layout in Adwaita's style on Linux, and sheets and
  alerts for dialogs on macOS. None of it has been looked at on a Mac or a Linux desktop:
  the traffic lights' place in the 35 px bar, the alert on macOS 26 (whose alerts changed
  alignment) and the buttons under KDE (Adwaita's look in Breeze's place) need a look. The
  menus are in macOS's menu bar and shortcuts use Command there; neither has been tried on
  a Mac, nor has ⌘Z undoing a text field's typing through the menu bar
  (`document.execCommand`). Edit > Copy in the menu bar can't copy the document's selected
  text (⌘C and the selection's bar can). A tab dropped where two Lectrix windows overlap goes by the windows' frames, not
  by which one is on top (only the Windows build asks the system for the window under the
  cursor), and moving tabs between windows has not been run there. Nor has full screen
  (on macOS the system's own, with its animation and its menu bar at the top edge).
- **Show in folder** (2026-10-18, built, not run there): a recent file's Show in Finder is
  `open -R`; Linux's Show in folder asks the file manager over D-Bus (`dbus-send`,
  FileManager1's ShowItems) and opens the folder alone without it. Neither has been tried,
  nor how a desktop without `dbus-send` falls back.
- **Touchpad pinch on Linux** (2026-10-08, built, not yet tried with a real touchpad):
  WebKitGTK magnified the whole app. `platform/linux.rs` now stops the pinch on the
  webview widget's `event` signal (before WebKit's bubble-phase zoom gesture) and emits
  `touchpad-pinch`; `viewer/pinch.ts` hands it to the page as Ctrl+wheel. macOS still
  unhandled.
- **Scrolling on Linux at a fractional display scale** (2026-10-12). The user's laptop:
  2560x1600 at 120 Hz, scale 1.333 (1920x1200 logical; GTK draws the window at 200%,
  3840x2400). Measured on that screen with `tests/perf/wheel-linux.py` (real two-finger
  scroll through Mutter's remote desktop API, `LECTRIX_PERF=watch`), maximized, the
  user's own state: frames 22.0 ms on average (45 fps) before the fix below, 16.8-17.1 ms
  after (a steady 60 fps, p95 20-22 ms, none over 50 ms). Smooth on a 120 Hz screen needs
  8.3 ms; two caps outside Lectrix hold it at 60 (below). An earlier headless Mutter at
  3840x2400 and 60 Hz (not the laptop's screen, as first thought) gave 49-60 ms before,
  19 ms after; headless Mutter cannot show more than 60 fps (no CRTC, so WebKit's vblank
  timer falls back to a fixed 60 Hz).
  - Fixed, the cause: GTK 3 paints each frame of the window into a new window-sized image in memory
    (5760x3600 there, 83 MB) and fills the window's background colour into it before
    WebKit draws its frame over it through GL. Filling fresh memory every frame kept the
    UI main thread at 16% user and 60% system time (page faults). `perf` (user time only,
    `cpu-clock:u`; it works inside the toolbox on processes it starts, not attached to
    host ones) showed `gtk_window_draw` -> `pixman_fill`; per-thread `/proc` times showed
    the system share.
  - Fix: `platform/linux.rs` (`skip_covered_window_background`) draws the window's
    children itself and skips GTK's background, except until the page has loaded and for
    0.5 s after a size change, when WebKit's frame may not cover the window. GTK still
    sets the opaque region from the colour. Startup and a live resize (Alt+F8), recorded
    through Mutter's screen cast, look as before.
  - Earlier: WebKit gets an accelerated canvas (`willReadFrequently` only on Windows,
    `RenderedImage.svelte`): blank pages worst 397 ms -> about 90 ms, first page
    345 -> 150 ms. No effect: passive wheel listeners, `will-change: transform`, skipping
    text geometry, `img.decode()`, `GDK_GL=always`. Worse:
    `WEBKIT_DISABLE_DMABUF_RENDERER=1`, `WEBKIT_SKIA_ENABLE_CPU_RENDERING=1`.
  - **Cap 1, WebKit:** `PreferPageRenderingUpdatesNear60FPS` (a WebKitGTK feature,
    on by default) holds page updates near 60 fps although WebKit's `VBlankMonitor`
    follows the display at 120 Hz through DRM. Turned off since 2026-10-12
    (`render_at_display_rate` in `platform/linux.rs`; WebKitGTK 2.42+, looked up at run
    time, since the `webkit2gtk` crate has no binding): 14.1-14.3 ms on average, 24-29% of
    frames one refresh apart (p50 16 ms). The user judged it better by eye. These are
    animation-frame times; the screen still gets at most 60 new frames a second (cap 2),
    so the gain is likely fresher frames rather than more of them.
  - **Cap 2, GTK 3:** a GTK 3 window drawn with GL never requests Wayland frame
    callbacks: `gdk_window_impl_wayland_end_paint` drops `pending_commit` when the paint
    used GL (`eglSwapBuffers` commits), so `on_frame_clock_after_paint` requests none,
    and `GdkFrameClockIdle` falls back to `FRAME_INTERVAL` (16667 µs). A bare
    `GtkGLArea` runs at 59-60 fps, the same window drawn with cairo at 113-119;
    `WAYLAND_DEBUG` shows no `wl_surface.frame` from the GL window. WebKitGTK's GTK 3
    window paints its frames through GL in that frame clock. GTK 4 is not affected the
    same way (Vulkan 119 fps; its GL paths measured 58-87 here, not investigated), and
    Papers, Fedora's PDF reader, is a GTK 4 app on Vulkan. `GDK_GL=disable` lifts the cap
    but copies WebKit's frames through the CPU: scrolling falls to about 10 fps. No
    public GTK 3 call restores the frame callbacks, so Lectrix cannot work around it.
    Reported 2026-10-12: GTK issue 8461
    (https://gitlab.gnome.org/GNOME/gtk/-/work_items/8461, text in
    `docs/upstream/gtk3-wayland-gl-frame-clock.md`) and a comment on WebKit bug 199376
    (https://bugs.webkit.org/show_bug.cgi?id=199376, `docs/upstream/webkitgtk-60fps-comment.md`).
  - What is left: WebKit's web-process main thread is now the busiest (about 60% of a
    core while scrolling). About a fifth of it decodes page PNGs (`createImageBitmap`
    decodes on that thread in WebKit), a quarter updates rendering and compositing layers.
  - Build environment used: toolbox `lectrix-dev` (Rust 1.99.0, WebKitGTK/GTK 3 devel,
    clang, perf), with `RUSTUP_HOME`/`CARGO_HOME` in `~/.cache/lectrix-toolbox`.
- **Platform services:** the default author comes from `USER`. WebView2's memory target
  has no equivalent.
- **Packages** for macOS (dmg, ad-hoc signed, not notarized) and Linux (AppImage and deb,
  plus rpm for final versions; built on Ubuntu 24.04, so glibc 2.39 or later) are built and released (ADR 0011)
  but have not been checked as the Windows installers are (the manual checklist covers
  only the Windows ones). Their PDF association (2026-10-18, after users' reports that
  Ubuntu did not offer Lectrix for PDFs and that macOS opened nothing from Finder) is
  untested by hand: the deb, rpm and AppImage desktop file now declares
  `MimeType=application/pdf;` and takes files (`Exec=lectrix %F`,
  `src-tauri/linux/lectrix.desktop`), the metainfo names the media type, and the macOS
  bundle declares PDFs (`CFBundleDocumentTypes`, rank Alternate, `tauri.macos.conf.json`)
  and opens the files macOS hands over (`RunEvent::Opened`, which only compiles on macOS,
  so CI's macOS job is its only build check).
  Crash recovery, the single-instance hand-off and the in-app updater are
  platform-neutral but untested there.
- **macOS versions** (read from the 1.0.0 bundles, 2026-10-12): both declare macOS 10.13
  as their minimum (`LSMinimumSystemVersion`, Tauri's default; the Apple Silicon binary
  itself asks for 11.0), which nothing backs. The Apple Silicon build is made on macOS 26
  (`macos-latest`), where CI also runs the tests; the Intel build is made on macOS 15
  (`macos-15-intel`) and no test runs on Intel. The app has not been started by hand on
  either, and no older macOS has been tried: the README names 26 and 15. Tailwind 4's CSS
  needs Safari 16.4's WebKit, which macOS has from 13.3, so the interface is unlikely to
  draw properly before that; `bundle.macOS.minimumSystemVersion` is not set to say so.
- **Linux software stores** (2026-10-12, not yet seen in a store): the 1.0.0 rpm, installed
  on Fedora, showed no icon in GNOME Software. The package had the desktop file and the
  icons (32, 128 and 256@2 px in hicolor) but no AppStream metainfo, and GNOME Software
  lists installed apps from metainfo alone (`component/description/..` in its
  `gs_appstream_add_installed`), so it showed the bare package. The deb and the rpm now
  install `org.lectrix.pdf.metainfo.xml` (`appstreamcli validate-tree` passes on the
  1.0.0 deb's files with it added; its only remark, a pedantic one, is the missing
  `<releases>`). The first release that carries it is the first real test. It has no
  screenshots, so a store's page shows none. The AppImage does not carry it.
- **License notices:** `THIRD_PARTY_LICENSES.md` lists the crates of the Windows build
  only; the macOS and Linux builds include more (GTK and WebKitGTK bindings, objc2 and
  others), which it does not name.
- **Tests:** some are Windows-only by design (DirectWrite fonts, locked files). Every
  test program runs (`--no-fail-fast`), and qpdf is installed on both, so the structural
  check runs there too.

## Releases and updates

- **Version:** `1.2.0`, after `1.1.0` (ADR 0011).
- **Installing a downloaded update has not been run.** Checking, the notice, progress,
  Stop, Don't ask again and rejecting a badly signed download were tried against a local
  test server; installing and restarting need a release signed with the real key. The
  first real release is the first full test.
- **No code signing** (Authenticode, Apple notarization): SmartScreen and Gatekeeper warn
  on the first manual install (ADR 0011).
  - **Plan for Windows** (decided 2026-10-12, not started): sign through SignPath
    Foundation, which signs open-source projects for free with its own certificate, so
    the publisher shown is "SignPath Foundation". Before applying: ask them whether the
    app icon and the PDF file icon, which are all rights reserved
    (`THIRD_PARTY_NOTICES.md`), break their rule against proprietary components, and
    publish the code signing policy they require. Then the Release workflow signs the app
    and its installer. A signature names the publisher; SmartScreen's warning still
    fades only as downloads build reputation. Not chosen: Certum's open-source
    certificate (from 25 EUR, in the author's name, its key on a card or in their cloud)
    and Azure Artifact Signing (individuals in the USA and Canada only).
  - **macOS: nothing decided.** The only route is the Apple Developer Program (99 USD a
    year) for a Developer ID certificate and notarization.
  - The README's "Download and install" says the builds are not signed; change it when
    either is done.
- **Administrators answer a UAC prompt on every Windows update**, per-user installs
  included: the NSIS installer offers "just me" or "everyone" and asks for the highest
  rights when it starts (ADR 0015). Only users without administrator rights update with
  no prompt. Updating `1.0.0-beta.1`, which installed per user without recording it,
  keeps it in `%LOCALAPPDATA%\Lectrix`.
- **The MSI is gone after 1.1.0** (ADR 0020, 2026-10-13). A copy installed from it
  updates through the setup program, which removes the MSI install first. CI's installer
  check runs the takeover on the released 1.1.0 MSI, with and without the PDF
  registration, and it passed in every run of 2026-10-13. Not yet done: an update started
  from inside an MSI-installed 1.1.0, now that 1.2.0 is out
  (`docs/manual-checklist.md`). Its limits (a silent install over an MSI, an MSI in a
  folder of the user's choosing, a user without administrator rights) are in the ADR.
- **Tests that failed only on CI, or only sometimes** (2026-10-13: 1.2.0 took five tries on CI).
  The first run failed four end-to-end tests that had only ever run on the author's
  screen: CI's runners ask for reduced motion and their window is 1028 px wide. The
  next three each failed one test that passes most of the time (a click that closes the
  window before the driver answers; a wheel over a page the grid has no cell for; a
  Rust test writing a file a session had closed a moment before), and once GitHub gave
  the macOS job no runner. All were faults of the tests. Since then:
  `npm run test:ci --prefix tests/e2e` shows the app as CI does (it fails the four tests
  as they were, with CI's messages), the tests run beside an open Lectrix, the soak run
  lists the tests that fail only sometimes (`.github/workflows/soak.yml`), and CI's
  Windows checks are separate jobs, so one that failed is run again alone. Known and
  left: `recovery.test.mjs` ends with a click that quits Lectrix, as
  `window-placement.test.mjs` did; no soak run has been done yet, so other such tests
  are likely.

## Upstream work

- `docs/upstream/mupdf-1.27.2-jpx-reduced-decoding.patch`: decoding JPEG 2000 at the
  resolution drawn, for Artifex. Not yet sent. Its comments say "Lectrix:"; reword them
  before sending.
- MuPDF's saved journal cannot be loaded back (`pdf_deserialise_journal` and
  `pdf_add_journal_fragment` disagree; ADR 0001). This could be reported.
- After an undo, MuPDF's incremental save writes the trailer's `/Size` from the edit
  section's length, which still counts the objects the undone step created, though it
  writes none of them (`writexref` in `pdf-write.c`). Lectrix trims those entries before
  every incremental save and recovery copy (`ffi::trim_unused_objects`); the fix belongs in
  MuPDF and could be reported.
- An incremental save that is not kept leaves the document pointing at it: MuPDF sets
  `doc->startxref` to the update it wrote, so a caller that writes to a temporary file
  and fails to move it into place writes a self-referring `/Prev` next time (ADR 0003, "A
  save that fails"). And a snapshot of a document with a cross-reference stream leaves a
  "Save document" step in the journal, although a snapshot is meant to leave the document
  as it was. Lectrix works around both in the shim; both could be reported.
- GTK 3 windows drawn with GL on Wayland update at 60 fps on faster displays, which holds
  WebKitGTK (and Lectrix's scrolling on Linux) to 60. Reported by the user on 2026-10-12:
  GTK issue 8461 (https://gitlab.gnome.org/GNOME/gtk/-/work_items/8461), and a comment on
  WebKit bug 199376 (https://bugs.webkit.org/show_bug.cgi?id=199376). Texts in
  `docs/upstream/`. When GTK fixes it, measure again with `tests/perf/wheel-linux.py` on a
  120 Hz screen.
- `pdf_get_xref_entry_aux` (`pdf-xref.c`, MuPDF 1.27.2) reads the section an object was
  last found in from `doc->xref_index` and then starts at section 0 all the same (`else
  j = 0`), going through every run of every section before it. A file whose last update
  has a thousand runs (1,310 objects scattered over 77,000) makes every lookup of an
  older object a thousand steps: edits in Lectrix are five times slower after such a
  save (the gap is under "Organizing pages" above). Using the index, or keeping a
  section's runs in an order that can be searched by halves, would mend it; this could
  be reported, with a file made by `pdf-cli pages move` and a save.
- The vendored `mupdf` crate's patches (`third_party/mupdf-rs/LECTRIX_PATCHES.md`) could be
  offered to mupdf-rs as a pull request, if the user wants.

## Names kept from the build

- The interop suites keep their build-phase names (`run.py phase0` to `phase5`,
  `phase4-local`, `phase5-local`); CI calls them by these names. Suites added since are
  named for what they check (`pages`).
- The `mupdf-rs` fork's branch is `folio-mupdf-1.27.2` and its patch file is
  `folio_patches.rs`, from the app's earlier working name. Renaming them needs a change in
  the fork first.
