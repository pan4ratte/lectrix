# Status

**v1 is complete (2026-10-04).** This file is the short index a session reads (AGENTS.md
section 10): where Lectrix stands, and one line for each known gap. The detail is in the
files under [docs/status/](status/). Keep both current: a gap found gets its line here and
its detail there; a gap closed leaves both.

v1 contains everything in AGENTS.md sections 1 and 6; what came since is in `CHANGELOG.md`
and in [releases.md](status/releases.md), "What v1 contains". The build's phase reports
are in git history (`git show 565fdb1:docs/progress.md`); ADRs that cite
`docs/progress.md` refer to that file.

## Performance

Measured 2026-10-13 (release build, on battery, 2560 x 1600 at 1.5x scaling): every
target of AGENTS.md section 2 is met, except that a JPEG 2000 scan's page comes blank for
0.26 to 0.4 s when scrolled to fast or jumped to. The table, why some numbers moved, the
libjpeg-turbo gain and the scripts: [performance.md](status/performance.md).

## Known gaps

In full, with measurements: [known-gaps.md](status/known-gaps.md).

- **A cover that is a whole jacket in one large image** shows the first page late (371 to 387 ms on mains).
- **JPEG 2000 scans** take about 0.25 s a page; scrolled fast or jumped to, a page comes blank.
- **Scroll modes** are not remembered per file; right-to-left documents' first page is on the left in two-page modes; a selection can't run into the next row.
- **PDF/A** compliance is neither checked nor written, so saves that may break it remove the claim; not tested on a real PDF/A file.
- **Annotations the list filters out** still show in thumbnails and in the page's text.
- **Windows:** a tab dragged out shows nothing outside its window and drops at the end of the row; panes and thumbnail size are shared; search, selection and tool stay behind; windows are not reopened; an update put off in one window is unknown to the others.
- **Full screen:** the switch holds frames 40 to 100 ms; the pane strip is 4 px; not remembered; a real Alt key is untried.
- **Visual adjustments:** inversion is approximate; pages with annotations take a second image; no commands; scrolling with them not measured.
- **Links** answer by `/Rect` only and need a pointer; only http and https open; bookmarks have no tooltip saying where they lead.
- **Undo history restarts** after each save and after a crash restore (MuPDF can't load a journal).
- **Recovery copies of large damaged files** are full copies (1.2 to 1.5 s).
- **Organizing pages:** edits are slower after a save that wrote many scattered objects (MuPDF's lookup); bookmark-heavy files; a deleted page stays after an ordinary save; limits of pages turned on screen, extracts, documents without a file and the Page labels dialog; parts not yet checked by a person.
- **Combining** drops the structure tree, threads, document JavaScript, open actions, viewer preferences and XMP; signatures become invalid.
- **Replies in Acrobat and Foxit** not yet checked by a person.
- **Repair and PDFgear:** a report that PDFgear lost annotations after repair, not re-tried since the fix.
- **Marks Lectrix 1.0 made on right-to-left text** are a character short at each end.
- **Text marked across both directions:** selecting it not yet checked by a person.
- **Accessibility:** making, moving and resizing annotations and starting a selection need a pointer; page text is not exposed to screen readers; no high-contrast handling.
- **Not covered by automated tests:** drag-and-drop from Explorer, touch and stylus, touchpad pinch, a tab dragged between windows.
- **Touchscreen pinch** does not zoom.
- **Test corpus:** `tests/corpus/` is empty; the local corpus lacks CJK text, signed files, UserUnit pages and some annotation types.
- **Engine differences** no file can change are in `docs/interop-profile.md`.
- **A change of a document's protection is lost in a crash.**
- **Document properties:** one language for title and description, one creator item; "Fast web view" after an incremental save.
- **Compress:** scans gain little (JBIG2 and MRC come with OCR); some images are not downsampled or are left as they are; one threshold for black and white; not yet compared with Acrobat and FineReader by a person.
- **Print:** on Windows each sheet is an image (large spool jobs); no pages per sheet, booklets, posters or tray by page size; the preview's greys are the webview's; not yet on a paper printer; landscape duplex edges to be settled.

## Split view

A joined tab moved to another window arrives as tabs of their own; the lock and a split
are not remembered; synchronized scrolling counts pages, not points. In full:
[known-gaps.md](status/known-gaps.md), "Split view".

## Platform gaps (macOS and Linux)

CI builds the app and runs the tests on both without failing the job; nothing has been run
interactively there. In full: [platform-gaps.md](status/platform-gaps.md).

- **Printing** through CUPS is built but not run.
- **Fonts:** no index of installed fonts (ADR 0005); non-embedded CJK text has no glyphs.
- **Window:** the title bar, menu bar, sheets, the selection handed to the system, tabs between windows and full screen are not looked at there.
- **Show in folder** and **touchpad pinch on Linux** are built, not tried; pinch on macOS is unhandled.
- **Scrolling on Linux** is held to 60 fps by GTK 3 and WebKitGTK (reported upstream).
- **Packages** and their PDF association are untested by hand; the macOS minimum version is unbacked; Linux store metainfo is untested.
- **License notices** name only the Windows build's crates; **some tests** are Windows-only.

## Releases and updates

Version `1.2.0`. In full: [releases.md](status/releases.md).

- Installing a downloaded update has not been run with a real release.
- No code signing: Windows through SignPath Foundation is decided, not started; macOS undecided.
- Package managers: winget and Flathub decided, not started.
- Administrators answer a UAC prompt on every Windows update.
- The MSI is gone after 1.1.0; an update from inside an MSI-installed 1.1.0 is not yet tried.
- Tests that failed only on CI or sometimes: what was changed, and the one left (`visual-adjustments.test.mjs` in full screen).

## Planned tools

Unlock, Protect, Document properties and Compress are built; Compare, then OCR with
compression for scans, come next. In full: [planned-tools.md](status/planned-tools.md).

## Upstream work

Bugs found in MuPDF, GTK, WebKitGTK and Svelte, and patches to offer, with what was sent:
[docs/upstream/README.md](upstream/README.md).

## Names kept from the build

- The interop suites keep their build-phase names (`run.py phase0` to `phase5`,
  `phase4-local`, `phase5-local`); CI calls them by these names. Suites added since are
  named for what they check (`pages`).
- The `mupdf-rs` fork's branch is `folio-mupdf-1.27.2` and its patch file is
  `folio_patches.rs`, from the app's earlier working name. Renaming them needs a change in
  the fork first.
