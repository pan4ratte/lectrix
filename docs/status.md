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
or Dark appearance), an About dialog with a button to the AGPL source on GitHub, and NSIS and MSI
installers that can register Lectrix for PDF files (ADR 0007); the NSIS one installs for
the user or for everyone (ADR 0015).

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
adjustments (brightness, contrast, inverted colours, greyscale) for a document's pages,
on screen only and remembered per file. The pages next to those on screen are rendered
ahead, so that pages slow to render (scans) are there when scrolled to.

## Performance

Last measured 2026-10-04: release build, PNG page images, a mid-range laptop.

| Target (AGENTS.md section 2) | Measured | Met? |
| --- | --- | --- |
| Window visible < 1 s | 30 to 99 ms | Yes |
| First page visible < 1 s | 1,000-page file 73 to 80 ms; 2,881-page dictionary 137 ms; a 33-megapixel JPEG 2000 scan 376 to 382 ms | Yes |
| No blank page > 200 ms while scrolling | 1,000-page file: none at steady speed, worst 18 ms fast, 46 to 54 ms jumping | Yes, except JPEG 2000 scans (below) |
| `lectrix.exe` < 200 MB idle (ADR 0002) | 63 to 64 MB with the 1,000-page file, 101 MB with a JPEG 2000 book | Yes |
| WebView2 growth rule (ADR 0002) | Whole process tree 635 to 647 MB idle; the growth limits held when last checked | Yes |

Scripts: `tests/perf/measure.ps1` and `tests/perf/memory-over-time.ps1`.

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
  and the pages are rendered and cached as before.
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
- **Organizing pages** is being built (plan agreed 2026-10-13: a mode of the document's
  tab with a page grid, where pages are selected, reordered, rotated, deleted, extracted
  and inserted, and page labels are edited). Done so far: the engine's `MovePages` and
  `DeletePages` operations and extracting pages into a new file (ADR 0018), reachable
  from `pdf-cli pages move`, `pages delete` and `pages extract` only; the app has the
  `extract_pages` command, and no interface for any of it yet. What they leave open:
  - **A deleted page stays in the file after an ordinary save.** Saves are incremental
    (AGENTS.md section 5.4), so the original bytes, the deleted page among them, remain
    and can be recovered. Only Save As (optimized) writes a file without it (tested).
    Decided 2026-10-13: the notification after deleting pages says so and offers Save As
    (optimized). It comes with the interface; nothing tells the user yet.
  - An extract carries every attached file of the document, whatever pages are picked,
    and nothing of its metadata (title, author, XMP), as a combined file does. While
    pages are extracted, the document renders nothing new.
  - A tagged file keeps its structure elements for deleted pages (they point at emptied
    page objects), and article threads and an open action that lead to a deleted page are
    left as they are. Deleting pages therefore removes a PDF/A claim at the next save.
  - A page tree that MuPDF and Lectrix read differently, or one with a page listed twice,
    is refused (`Error::DamagedPageTree`) and not mended. None of the 30 local corpus
    files is refused.
  - Measured 2026-10-13 on the local corpus, release build, the session's own re-reading
    included: moving or deleting pages takes 15 to 20 ms in the 2,881-page dictionary and
    under 35 ms in most files. The 2,597-page file with the most bookmarks takes 0.19 to
    0.25 s, where rotating a page (an edit v1 has) takes 0.15 s: nearly all of it is the
    session reading the outline and the named destinations again after every edit.
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

## Platform gaps (macOS and Linux)

CI builds the app and its tests on both, and runs the tests without failing the job.
Nothing has been run interactively there.

- **Fonts:** no index of installed fonts (ADR 0005). Non-embedded fonts other than the
  base 14 render with MuPDF's substitutes, and non-embedded CJK text has no glyphs. This
  needs fontconfig (Linux) and Core Text (macOS) behind the existing `SystemFonts` trait.
- **Window:** the title bar's window buttons follow Windows (macOS puts them on the
  left). A tab dropped where two Lectrix windows overlap goes by the windows' frames, not
  by which one is on top (only the Windows build asks the system for the window under the
  cursor), and moving tabs between windows has not been run there. Nor has full screen
  (on macOS the system's own, with its animation and its menu bar at the top edge).
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
  only the Windows ones). No PDF file association is configured for them.
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

- **Version:** `1.1.0`, after `1.0.0` (ADR 0011).
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
    and both installers. A signature names the publisher; SmartScreen's warning still
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
- The vendored `mupdf` crate's patches (`third_party/mupdf-rs/LECTRIX_PATCHES.md`) could be
  offered to mupdf-rs as a pull request, if the user wants.

## Names kept from the build

- The interop suites keep their build-phase names (`run.py phase0` to `phase5`,
  `phase4-local`, `phase5-local`); CI calls them by these names. Suites added since are
  named for what they check (`pages`).
- The `mupdf-rs` fork's branch is `folio-mupdf-1.27.2` and its patch file is
  `folio_patches.rs`, from the app's earlier working name. Renaming them needs a change in
  the fork first.
