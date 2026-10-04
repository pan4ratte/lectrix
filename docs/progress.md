# Progress reports

## Phase 0: Feasibility spike (report, 2026-10-03)

**Status: complete.** Every box is checked, and CI passed on GitHub (run 37073273603,
`windows-latest`, all steps green, installers uploaded as an artifact). The first run took
79 minutes, mostly compiling MuPDF from source three times (clippy, tests, release build);
later runs reuse the Rust cache. Reviewed by the user on 2026-10-03 (decisions below).

### Checklist

| Item | Result |
| --- | --- |
| Tauri 2 + SvelteKit (static, SSR off) + Tailwind skeleton builds and runs on Windows | Done. Release build `target/release/lectrix.exe`. |
| MuPDF linked through the `mupdf` crate; version pinned and recorded | Done. MuPDF 1.27.2. The crate is a vendored, patched copy at upstream `537d505`; see ADR 0001 and `docs/versions.md`. |
| A page renders in the window through `lectrix://`; render and encode times measured on a 500-page file | Done. See the performance table below. |
| `pdf-cli` writes page labels, writes an outline, merges two files, adds a highlight with an appearance stream | Done: `labels set`, `outline set`, `merge`, `annot markup`. |
| Those four outputs pass `qpdf --check` and display correctly in the harness (all three engines) | Done. `python tests/interop/run.py phase0`: 78 checks pass, 0 fail. |
| Journalling works on at least one operation | Done. `ffi::Journal` (enable, undo, redo, state, step names) through a C shim; test `undo_and_redo_an_annotation`. |
| ADR on crate coverage, `ffi/` wrappers, thread safety | Done. `docs/decisions/0001-mupdf-binding-coverage.md`. |
| Performance numbers measured and reported | Done (below). One target is not met; another depends on a definition. |

### Interop harness summary

The harness renders with MuPDF (`pdf-cli`, the same MuPDF build as the app), PDFium
(pypdfium2 / PDFium 153) and pdf.js (pdfjs-dist 6.3). Phase 0 cases:

- **Labels:** roman, then decimal, then a prefixed range. PDFium and pdf.js read back
  exactly the expected 20 labels.
- **Outline:** three levels with open and closed items and a UTF-16BE title
  ("Préface — ünïcödé 日本"). Identical tree in PDFium and pdf.js.
- **Merge:** three sources (one with labels, one with an outline, one plain). 45 pages,
  labels kept per source, outlines nested under each source's title, in both engines.
- **Highlight:** with a note, at 60% opacity, on normal, 90/180/270-rotated, offset-CropBox,
  CropBox + rotation and UserUnit = 2 pages. It is visible in all three engines (about 75%
  of `/Rect` pixels changed), and the engines agree (colour distance ≤ 12/441, coverage
  ratio ≤ 1.03).

Engine finding: **PDFium ignores `/UserUnit`** (it renders the UserUnit = 2 page at half the
size MuPDF and pdf.js use). The annotation still lands on the right text in every engine.
Recorded in `docs/interop-profile.md`.

`mutool draw` is not installed. The MuPDF leg uses `pdf-cli render`, which runs the same
pinned MuPDF the app ships. This is equivalent, but it is a small deviation from section 9's
wording.

### Tests

- `cargo test --workspace`: 39 tests. They include geometry against MuPDF's own page
  transform (every rotation, offset CropBox, UserUnit), label/outline/highlight/merge round
  trips checking every profile key, journal undo/redo, incremental-save byte prefix, and
  a locked target left untouched.
- `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --check`,
  `svelte-check`, Vitest: clean.

### Performance (section 2 targets)

Machine: this development laptop, Windows 11, release build. File: a generated 500-page
text PDF (`pdf-cli gen --pages 500`). The generated file is deliberately simple: real books
with images and embedded fonts will render more slowly, so these numbers need repeating on
corpus files once the user supplies them.

| Target | Measured | Met? |
| --- | --- | --- |
| Window visible < 1 s | Window handle 33–143 ms after launch; UI mounted 508–692 ms after `main` (3 runs) | Yes |
| First page of a 500-page PDF visible < 1 s | 1,300–1,607 ms from UI ready to image shown | **No**, see finding 1 |
| No blank page > 200 ms while scrolling | Not measurable yet (no continuous scroll until Phase 1). Per-page cost after warm-up: 2.9 ms render + 2.1 ms encode at 100% zoom; 10.2 + 6.2 ms at 200% (mean of 25 pages) | Phase 1 |
| Idle memory < 200 MB, one large document open | `lectrix.exe` 60 MB working set (13 MB private). Including the WebView2 processes: about 500 MB | Depends on the definition, see finding 3 |

Reproduce: `powershell -ExecutionPolicy Bypass -File tests/perf/measure.ps1 -Pdf <file>`,
and `pdf-cli bench <file> --scale 1.333`.

### Findings and decisions needed

1. **First-page time is dominated by one-time system font enumeration (about 1.6 s).**
   With the `mupdf` crate's `system-fonts` feature, the first non-embedded font (here
   base-14 Helvetica) makes `font-kit` enumerate the Windows font collection. With
   `system-fonts` disabled, the same first page is ready in **15 ms** (display list 5 ms,
   raster 5 ms, encode 2 ms). Options:
   - (a) Drop `system-fonts`. Fastest, but non-embedded non-base-14 fonts (for example a
     PDF naming "Arial" without embedding it) render with MuPDF's generic substitutes, and
     non-embedded CJK text would have no glyphs.
   - (b) Keep it, warm the font cache on a background thread at startup, and patch the
     vendored crate so base-14 names go straight to MuPDF's built-in, metric-compatible
     fonts (Acrobat behaves the same way). Only documents with other non-embedded fonts
     would pay the cost, and the warm-up usually finishes before the user opens a file.
   - **Recommendation: (b)**, done in Phase 1.
2. **PNG encoding is above the 30% threshold.** Encoding takes 61–71% of render time on
   these pages. Section 3 says to switch to raw RGBA drawn into a `<canvas>` in that
   case, so Phase 1 will do that unless you object.
3. **Memory target definition.** Lectrix's own process uses about 60 MB. WebView2's browser,
   GPU and renderer processes add about 440 MB, and we can only partly control that. Should
   the 200 MB target cover `lectrix.exe` only, or the whole process tree including WebView2?
4. **In-place save is blocked on Windows (verified).** MuPDF keeps the source file open
   without `FILE_SHARE_DELETE`, so the atomic rename onto the open file fails (reported
   cleanly as "file is open in another program"). Phase 1 will open files through a stream
   that allows the rename, or from memory; I'll write an ADR then.
5. **Vendored `mupdf` crate (ADR 0001).** 0.8.0 does not build on Windows with current
   LLVM, and it exposes no raw handles for `ffi/`. The patch is small (`src/raw.rs`). It
   could be offered upstream as a pull request; I will only do that if you want it.
6. **SvelteKit 3 / TypeScript.** SvelteKit 3 requires TypeScript 6 (TypeScript 7 is not
   supported yet) and replaces `$lib` with `#lib/*` subpath imports. Recorded in
   `docs/versions.md`.

### Review decisions (2026-10-03)

1. **Fonts: option (b).** Phase 1 warms the system font cache on a background thread at
   startup, and the vendored crate sends base-14 names straight to MuPDF's built-in fonts.
2. **Memory target: decided in ADR 0002.** 200 MB applies to `lectrix.exe`; the whole tree
   including WebView2 is reported every phase, with a growth limit.
3. **Page images: follow section 3 and switch to raw RGBA drawn into a `<canvas>`.**
   Raw pixels are large: a page at 150% on a 2x display is about 14 MB. So Phase 1 measures
   the end-to-end time (request to pixels on screen) for both formats on the same pages. If
   raw RGBA turns out slower overall, that gets reported as a conflict with section 3
   rather than quietly keeping PNG.

### Not yet done (by design, later phases)

The real viewer (continuous scroll, tabs, thumbnails, search), the LRU cache and tiles,
the custom title bar with Mica, in-place outline editing, Phase 4 merge features (page
selection, link and name remapping), the remaining annotation types, repair, and
installers. CI is written but has not run.

## Phase 1: Viewer (report, 2026-10-03)

**Status: complete.** Reviewed by the user on 2026-10-03 (decisions below); CI passed on
the pushed commits (runs 37089408733 and 37092016269). The WebView2 memory rule moved to
the Phase 2 checklist. The original report follows.

*Report as written:* ready for review, not yet complete. The viewer, saving and file
handling work, and local checks are green. Three things stand between this and "done": CI
has not run on these commits (they are not pushed yet), WebView2 memory breaks the growth
rule in ADR 0002, and raw RGBA turned out slower than PNG, which conflicts with section 3.
Decisions 1 and 2 below need you.

### Checklist

| Item | Result |
| --- | --- |
| Everything in 6.1 works | Yes, with the gaps listed under "Not verified". Checked in the running app: open from the command line, a second launch adds a tab to the running window, tabs (reorder by drag, dirty dot, close prompt), continuous scroll, fit width/page, presets, Ctrl+wheel zoom at the cursor, tiles at high zoom, page box by label ("A-3") or number, thumbnails, back/forward, selection and copy (also in a rotated view), search with highlights, view rotation, Rotate pages with undo/redo, recent files, remembered view per file. |
| 1,000-page file: no blank page visible > 200 ms while scrolling | Yes for continuous scrolling: no blank page at 2,000 px/s, worst 122 ms at 6,000 px/s. Jumping to random places (like dragging the scrollbar) is borderline: worst 200–246 ms with RGBA (1 of 31 jumps over 200 ms), 171 ms with PNG. See the performance table. |
| Atomic save, dirty state, close prompts, external-change detection | Yes. Saving in place works while the file is open (ADR 0003), repeatedly, with qpdf-clean output. Closing a dirty tab or the window prompts. Another program replacing the file shows a banner (Reload, or Save As when there are unsaved changes). |
| Layout, theming and shortcuts from section 8 (viewer parts) | Yes: custom title bar with menus, tabs and window buttons; Mica with a solid fallback (Windows 10); system accent color with contrast-checked text; Pages sidebar; status bar "iv (4 of 312)", zoom, save state; section 8 shortcuts. Dark mode is not visually checked (see below). |
| CI green | Not yet: nothing pushed. |

### What was built

- **pdf-core.** Sessions now open files through a share-delete handle (a small C stream in
  the FFI shim) so the open file can be replaced, and reopen after each save (ADR 0003).
  Operations (`RotatePages` for now) are journal steps with names for the Edit menu;
  revisions follow the journal, so undoing back to the saved state clears the dirty dot.
  New: RGBA and 512 px tile rendering, an LRU image cache, structured text and search,
  document flags (signed, encrypted, repaired, permissions), passwords, reload.
- **Fonts (Phase 0 decision 1).** Done through the crate's public `set_font_loader` hook
  instead of patching the vendored crate: base-14 names go straight to MuPDF's built-in
  fonts, and the system font collection warms up on a background thread. First page went
  from 1.3–1.6 s to 72–160 ms.
- **App crate.** All IPC commands typed through ts-rs; paths never come from the webview.
  Drag-and-drop, command line, single instance, recent files, password prompts, a file
  watcher (stat polling every 1.5 s; no new dependency), a rotating log in
  `%LOCALAPPDATA%\org.lectrix.pdf\logs`, and a `platform` module (Mica, accent color, path
  identity).
- **Frontend.** Svelte 5 components under `src/lib/features/viewer/` and
  `src/lib/components/`; render requests are prioritized by distance from the viewport
  and dropped when a page scrolls away before its request starts.
- **New dependencies:** `tauri-plugin-single-instance` 2.5.2 (MIT OR Apache-2.0; on
  Windows only the plugin itself is compiled) and `windows-sys` 0.61.2 (already in the
  tree through Tauri). `docs/versions.md` and `THIRD_PARTY_NOTICES.md` are updated.

### Tests

- `cargo test --workspace`: 102 tests (Phase 0: 39). New: the share-delete stream
  (replace while open), RGBA vs RGB render equality, tiles reassembling a page, the image
  cache, text geometry and search (also on a rotated page), rotate/undo/redo with
  revisions, repeated in-place incremental saves, Save As, encrypted files keeping their
  encryption, a damaged file falling back to a full save, reload, labels, signature
  detection, the document registry (duplicate open, watcher, Save As clash), the app
  state store, the log rotation, the protocol parser.
- Vitest: 23 tests (layout, zoom ladder and fitting, page box with labels, history,
  selection hit-testing and copied text, page ranges, shortcuts, accent contrast).
- `cargo clippy -D warnings`, `cargo fmt --check`, `svelte-check`: clean. Interop harness
  (Phase 0 cases): 78 passed, 0 failed.
- The tests found one Windows hazard worth knowing: MuPDF writes files through the C
  runtime, whose handles are inheritable, so a child process started during a save keeps
  the file open. Lectrix starts no child processes; details in ADR 0003.

### Performance (section 2 targets)

Machine: this development laptop, Windows 11, 1.5× display scaling, release build. File:
the generated 1,000-page text PDF. Run: `tests/perf/measure.ps1 -Pdf big1000.pdf -Perf
[-Format png]`.

| Target | Measured | Met? |
| --- | --- | --- |
| Window visible < 1 s | 31–195 ms | Yes |
| First page visible < 1 s | 72–160 ms (Phase 0: 1,300–1,607 ms) | Yes |
| No blank page > 200 ms while scrolling (1,000 pages) | Continuous: 0 blank events at 2,000 px/s (46 pages); worst 122 ms at 6,000 px/s (139 pages). Random jumps: worst 200–246 ms (RGBA), 171 ms (PNG) | Yes for scrolling; jumps borderline |
| lectrix.exe < 200 MB (ADR 0002) | 105–108 MB idle with the document open; 137–141 MB after scrolling 185 pages | Yes |
| WebView2 growth ≤ 100 MB when opening a 1,000-page document (ADR 0002) | Empty window 507 MB (tree). With the document open and idle: 682–762 MB (+175 to +255). After the scroll tests: 1.6–2.0 GB, of which the WebView2 GPU process alone is 946 MB | **No**, decision 2 |

**RGBA versus PNG, end to end** (request to pixels on a canvas, same 16 pages at the
scale the window used, 2.485 px/pt):

| Format | End to end (mean) | Server time (mean) | Bytes per page |
| --- | --- | --- | --- |
| Raw RGBA (section 3) | 95–103 ms | 5–6 ms | 11.7 MB |
| PNG, fastest compression | 34–37 ms | 12–13 ms | 1.2 MB |

The time goes into moving 11.7 MB through WebView2's custom-protocol bridge, not into
rendering. Encoding is now cheap because the PNG is made from the cached RGBA render.

### Decisions needed

1. **Page image format: section 3 conflict (Phase 0 decision 3 asked me to report this).**
   RGBA is about 3× slower end to end than PNG here, and does not improve scrolling. The
   app still defaults to RGBA as section 3 says; `LECTRIX_IMAGE_FORMAT=png` switches it, and
   both paths share one cache.
   - (a) Make PNG the default and record it in an ADR. One-line change.
   - (b) Keep RGBA and move pixels through WebView2's shared-buffer API (no copies).
     Probably the fastest, but Windows-only, needs new `webview2-com` code behind the
     `platform` trait, and would be a project of its own.
   - **Recommendation: (a) now;** revisit (b) only if large scans prove slow.
2. **WebView2 memory breaks ADR 0002's growth rule.** Lectrix's own process is well under
   target; the growth is in WebView2, mostly its GPU process, and it keeps growing during
   long scrolls. Releasing canvases as pages leave the screen and turning off the browser
   cache for page images did not change the picture. Options:
   - (a) Spend part of Phase 2 investigating (fewer mounted pages, lower-resolution
     canvases while scrolling fast, `<img>` with object URLs instead of canvases, WebView2's
     memory target level), and report back.
   - (b) Revise the rule in ADR 0002 if it proves to be Chromium caching that the system
     reclaims under memory pressure.
   - **Recommendation: (a),** before more UI piles onto the canvas.
3. **Undo history after saving.** Saving reopens the file (ADR 0003), so undo history starts
   again after each save, as in Acrobat. Is that acceptable?
4. **Push to GitHub** so CI can run on these commits? I have not pushed anything.

### Not verified (please check during review)

- **Drag-and-drop from Explorer.** Implemented on the Rust side (paths never pass through
  the webview); I could not simulate an OLE drag from a script.
- **Dark mode and a touchpad pinch.** I did not change your system theme to test it; pinch
  arrives as Ctrl+wheel, which is tested.
- **The `.pdf` file association itself** is registered by the installer in Phase 6. Phase 1
  handles what the association does: launching with a path, or handing it to the running
  window.
- **Real-world files.** `tests/corpus/` is still empty; all numbers above come from a
  generated text PDF. Scanned and image-heavy books will render more slowly.
- **End-to-end tests** (`tauri-driver` + WebdriverIO, section 9) are not set up yet. They
  need msedgedriver matching the installed WebView2 and new dev dependencies. I suggest
  adding them at the start of Phase 2 for open, save and reopen.

### Behavior choices made without explicit guidance

- Fit width and fit page use the current page when they are applied, and re-fit only when
  the window width (or, for fit page, height) or the view rotation changes, not while
  scrolling past pages of other sizes.
- Search uses MuPDF's case-insensitive search in 32-page chunks, starts at the current
  page and wraps. A search jump is not recorded in back/forward history; page-box and
  thumbnail jumps are.
- Copying respects the document's copy permission; rotating pages respects the
  assemble/modify permission and a signed document's warning.

### Review decisions (2026-10-03)

1. **Page images: PNG by default** (ADR 0004). Raw RGBA stays available for measurement.
2. **WebView2 memory:** investigated in Phase 2 (added to its checklist in AGENTS.md).
3. **Undo history starts again after each save:** accepted (ADR 0003).
4. **CI:** the Phase 1 commits are pushed.
5. **Dark mode:** a light/dark/system switch in Settings, planned for Phase 5, where the
   Settings dialog is built (AGENTS.md sections 8 and 10).
6. **Real-world test files:** selected from the user's Calibre library into a git-ignored
   local corpus (see "Local corpus" below); the library itself is never touched.
7. **End-to-end tests:** added to the Phase 2 checklist.

### Local corpus (real-world files, 2026-10-03)

`pdf-cli survey` profiled all 554 PDFs in the user's Calibre library (5 GB) read-only in
54 s, with no failures. `tests/local-corpus/select.py` picked 30 files (705 MB), one or two
per category, copied into the git-ignored `tests/local-corpus/files/` (SHA-256 checked;
the library was only read). Covered: repaired (2 large scans), encrypted with permission
restrictions (2), forms (2), right-to-left text, pages rotated 90/180/270 (some and all),
offset CropBox, scanned (small, 1,020 pages, with ink), 2,881 pages, 5,023 bookmarks, deep
outline with labels, 1,311 sticky notes, and annotations made by other apps (Highlight,
Underline, StrikeOut, Text, Ink, FreeText, Stamp, Square, Caret). **Not in the library:**
CJK text, signed files, UserUnit pages, Squiggly, Circle and file attachments. These
still need files from elsewhere.

**Round trip** (`cargo test --release -p pdf-core --test local_corpus -- --ignored`): all
30 files open, render, extract text and find a word from page 1. A copy of each gets
page 1 rotated and is saved in place. Reopened, page count, labels, bookmarks and every
annotation are unchanged, and `qpdf --check` is never worse than for the original (12 of
the originals already have qpdf warnings). Both repaired files fell back to a full save, as
designed.

**Performance on real files** (app, PNG):

| File | First page visible | lectrix.exe |
| --- | --- | --- |
| 2,881-page dictionary | 107 ms; scrolling: no blank page at either speed, worst jump 173 ms | 108 MB (157 after scrolling) |
| 173 MB repaired scan | 315 ms | 120 MB |
| 1,020-page scan | 88–113 ms | 107 MB |
| Files with non-embedded fonts (before the fix below) | **4.9–6.0 s** | up to 203 MB |

**Finding: non-embedded fonts make first pages slow.** For a non-embedded font, MuPDF asks
the system font lookup (the `mupdf` crate's `system-fonts` feature, through `font-kit`).
On Windows, `font-kit` has no index: for each new font name it loads installed fonts one
after another and compares names, about 1.3 s per name, and it remembers the answer only
for that name. Before any fix, 21 of the 554 library files took over 1 s to show their
first page (13 over 2 s).

- Fixed now (commit "perf(fonts)"): aliases of the base-14 fonts that no installed font
  can match ("TimesNewRoman,Bold", "CourierNew", "Arial,Italic") go straight to MuPDF's
  built-in fonts, which MuPDF used for them anyway. Pages render pixel-identically (10
  real-world pages compared). Library files over 1 s: 21 → 18; over 2 s: 13 → 9.
- Still slow: real names of installed fonts (TimesNewRomanPSMT in 21 of the 40 files over
  500 ms, ArialMT, "Times New Roman", Tahoma, Segoe UI, Palatino...). These do resolve,
  but only after the slow scan. 6 of those 40 files make no font lookup at all; their
  first pages are simply heavy.
- Proposed fix (decision needed): build our own index of installed fonts once, on the
  background warm-up thread, through DirectWrite (family, style and PostScript name
  without loading font files), and use it instead of `font-kit`. That means turning off
  the crate's `system-fonts` feature, which also drops `font-kit` from the build. It
  needs a `FontIndex` behind the `platform` idea (DirectWrite on Windows; fontconfig and
  Core Text later), the CJK fallback families the crate handled, and an ADR.

## Phase 2: Bookmarks (report, 2026-10-03)

**Status: complete.** Reviewed by the user on 2026-10-03 (decisions below); Acrobat
checked by the user; CI green, including the end-to-end tests, twice on the same commit
(run 37135881818 and its re-run). Getting the end-to-end tests to run on CI took three
fixes, described under "CI" at the end of this report. The original report follows.

*Report as written:* ready for review, not yet complete. Bookmarks work in the app and
every local check is green, including the new end-to-end tests. Four things stood between
this and "done": CI had not run on these commits (not pushed), Acrobat had to be checked by
you, the WebView2 growth rule was still not met (a revision is proposed), and one
real-world file still takes 1.8 s to show its first page for a reason unrelated to fonts.

### Checklist

| Item | Result |
| --- | --- |
| Everything in 6.2 works, with undo/redo for each action | Yes. Bookmarks panel (tree read from the outline, expanded state read and written), Ctrl+B at the current view with selected text as the title, inline rename (F2, double-click), drag to reorder and nest, Alt+Shift+arrows, delete (confirmation when it has children), "Set destination to current view". Add, rename, move, delete and retarget are each one undo step ("Undo move bookmark"). New and retargeted bookmarks get `[page /XYZ left top null]`; non-ASCII titles are UTF-16BE with a BOM. |
| A 3-level outline edited in the app opens correctly in Acrobat, Edge and Firefox | Edge (PDFium) and Firefox (pdf.js): yes, `python tests/interop/run.py phase2`, 15 checks: tree, titles, target pages, expanded states and `/XYZ` positions. **Acrobat: needs you** (files below). |
| Untouched bookmarks keep their destinations byte-for-byte (test) | Yes, with one precision (decision 4). Test `edits_touch_only_what_they_must_and_untouched_destinations_survive` on a hand-written file: an incremental save rewrites exactly the edited items, the neighbours whose links changed, the parents whose counts changed and the outline root; every other item keeps its original bytes. Neighbours that MuPDF rewrites keep the same destination and action values, but MuPDF writes the dictionary in its own spelling (`72.000` becomes `72`, spaces go). |
| E2E tests (tauri-driver + WebdriverIO) in CI | Written and passing locally (two flows, 5 s). Added to the CI workflow, which has not run yet. |
| WebView2 memory investigated | Yes: growth on opening halved, its sources found, the post-scroll figure explained. The rule is still not met: decision 1. Details in ADR 0002. |
| Font index replaces `font-kit` (ADR first) | Yes: ADR 0005, then DirectWrite index; `font-kit` and `dwrote` are gone from `Cargo.lock`. Same fonts as before: 70 first pages pixel-identical. First page under 1 s for 4 of the 5 corpus files with non-embedded fonts; the fifth is decision 2. |

### What was built

- **pdf-core.** `outline/` reads the outline as a tree with stable ids (object numbers) and
  targets resolved to a page and a point: explicit destinations, named destinations
  (`/Dests` and the name tree), GoTo, URI, GoToR and other actions, styles. Damaged
  outlines (cycles, shared or direct items) are shown but not edited. Edits write only the
  keys whose values change. Expanded states from the panel are kept by the session and
  written at the next save through an implicit journal operation (new shim call), so
  expanding neither dirties the tab nor adds an undo step.
- **Fonts.** `fonts/index.rs` (portable matching, CSS Fonts 3 like `font-kit`) and
  `platform/windows.rs` (DirectWrite system font set: 868 faces in 4 to 6 ms, no font file
  opened). Vendored crate patch 4 shares font data with MuPDF instead of copying it.
- **App.** Sidebar tabs (Pages, Bookmarks), the bookmarks panel (virtualized ARIA tree,
  pointer drag-and-drop; HTML5 drag events do not reach the webview while Tauri's file
  drop is on), the inspector, Document > Add bookmark, View > Bookmarks. Five bookmark
  operations and `set_bookmark_open` over IPC, types generated by ts-rs. `LECTRIX_OPEN`
  opens files like command-line arguments (tauri-driver hands launch arguments to WebView2
  on Windows). Page canvases are CPU-backed and fewer pages are mounted (memory); a
  minimized window sets WebView2's memory target to Low.
- **pdf-cli.** `outline show`, `outline edit --op ...` (through the same session path as
  the app), `fonts` (index, name resolution, a document's non-embedded fonts), rotated
  pages in `info`.
- **Tests and tools.** `tests/e2e/` (standalone webdriverio 9.32.0, Node's test runner,
  `fetch-edgedriver.ps1`), interop `phase2`, `measure.ps1 -Empty`, `LECTRIX_PERF=scroll`,
  `tests/perf/memory-over-time.ps1`.
- **New dependencies** (all MIT OR Apache-2.0 and already in the tree through Tauri):
  `windows` 0.62.2 in `pdf-core` (DirectWrite), `webview2-com` 0.39.1 and `windows-core`
  0.62.2 in the app. Test-only: webdriverio (MIT), tauri-driver (Apache-2.0 / MIT).
  Removed: `font-kit`, `dwrote` and their dependencies.

### Tests

- `cargo test --workspace`: 124 (Phase 1: 102), plus the ignored local-corpus round trip,
  which passes on all 30 files. New: named-destination lookup, reading every target kind,
  surgical edits with the exact set of rewritten objects, optimized save keeping values,
  undo/redo of bookmark edits, invalid moves, the first bookmark creating the outline,
  damaged outlines refused, font matching (PostScript, suffixes, CSS fallbacks, CJK) and
  installed Windows fonts.
- Vitest: 35 (Phase 1: 23): bookmark rows, insertion point, moves, drop zones, keyboard
  moves, selection after delete, titles and target descriptions.
- E2E: open, rotate, save, reopen; add (Ctrl+B), rename (inline, F2), nest by dragging,
  undo/redo, collapse, save, reopen, follow a bookmark; checked on disk with pdf-cli.
- Interop: phase0 78 passed, phase2 15 passed. `cargo clippy -D warnings`, `cargo fmt`,
  `svelte-check`: clean.
- Real files: `outline show` reads all 30 corpus outlines (the largest has 5,023 items, in
  0.2 s for the whole process) with no damaged outline and no unresolved destination; an
  edit round trip on that file passes `qpdf --check`.

### Font index results

The same 70 first pages (the 40 library files whose first page took over 500 ms, plus the
30 corpus files) were rendered before the change and after it. All 70 are pixel-identical.
Time for `pdf-cli render` of page 1 (process start to PNG written):

| | Before (font-kit) | After (index) |
| --- | --- | --- |
| Files over 1 s | 14 | 1 (no font lookups; a heavy page) |
| Files over 500 ms | 30 | 6 |
| Worst | 3,425 ms | 2,446 ms (the same heavy page: 2,348 ms before) |

In the app (first page visible, 3 cold starts each), corpus files with non-embedded fonts:
`11-forms` 77 to 82 ms, `22-rotated-270` 67 to 68 ms, `30-slow-first-page` 72 to 76 ms,
`01-annot-caret` 644 to 648 ms, `29-slow-first-page` 1,819 to 1,840 ms (decision 2).

### WebView2 memory (ADR 0002 rule)

Generated 1,000-page file; WebView2 = whole tree minus `lectrix.exe`.

| | Phase 1 | Phase 2 |
| --- | --- | --- |
| Empty window | 507 MB tree | ~418 MB WebView2 |
| Document open, idle: growth over empty | +175 to +255 MB | about +120 MB (one-page document: about +120 MB too) |
| After the scroll tests | 1.6 to 2.0 GB tree | ~1.0 GB tree (plateau; ~1.7 GB if the format comparison runs first) |
| Minimized after scrolling | not measured | ~0.84 GB tree |

Where it comes from, what helped and what did not: ADR 0002, "Phase 2 findings".

### Performance (section 2 targets)

Same machine and file as Phase 1 (`measure.ps1 -Pdf big1000.pdf -Perf`).

| Target | Measured | Met? |
| --- | --- | --- |
| Window visible < 1 s | 71 to 221 ms | Yes |
| First page visible < 1 s | 77 to 143 ms | Yes |
| No blank page > 200 ms while scrolling | Steady: none; fast: worst 56 ms; random jumps: worst 127 ms (Phase 1: 171 to 246 ms) | Yes |
| lectrix.exe < 200 MB | 63 MB idle (Phase 1: 105 to 108); 104 MB after scrolling | Yes |
| WebView2 growth ≤ 100 MB | about +120 MB | **No**, decision 1 |

### Decisions needed

1. **WebView2 growth rule.** What remains (+120 MB) is the cost of showing one page, and no
   longer depends on the document. Options:
   - (a) Revise ADR 0002's rule to what Lectrix controls: opening a large document may not
     cost more than 30 MB over opening a one-page document, and memory after scrolling
     must plateau (three rounds of the scroll tests within 10% of one round). Both hold
     today.
   - (b) Keep +100 MB and push further: render pages at a lower resolution than the screen
     (visibly softer text), or keep WebView2's memory target Low while visible (Microsoft
     warns this costs speed).
   - **Recommendation: (a).**
2. **`29-slow-first-page` takes 1.8 s.** Its fonts now cost 5 ms; page 1 is a 33-megapixel
   JPEG 2000 scan, and decoding it is the time (larger than MuPDF's 96 MB store, so it is
   not kept either). Making background work wait for the visible page made it slower
   (2.2 s), so I reverted that. Options: (a) accept it as content cost and list it under
   Phase 6 performance gaps; (b) look into decoding large JPEG 2000 images at the
   resolution shown, in Phase 6. **Recommendation: (a) now, (b) in Phase 6.**
3. **Push to GitHub** so CI runs (it now also runs the E2E tests and interop `phase2`)?
4. **Byte-for-byte.** Bookmarks the user did not touch keep their bytes exactly unless an
   edit changes their `/Prev`, `/Next` or `/Count`. In that case MuPDF rewrites the
   dictionary in its own spelling, with the same values. Is that what you meant, or should
   rewritten neighbours keep their original spelling too? That would mean serializing
   outline dictionaries ourselves instead of through MuPDF, which is a larger change.

### Please check in Acrobat

`target/test-output/manual/phase2/`: `outline-source.pdf` (as another app wrote it),
`outline-edited.pdf` (edited through the app's code: renamed, moved across parents,
deleted, added at levels 1 and 3, retargeted, expanded) and `app-bookmarks.pdf` (made in
the app by the E2E test). Expected for `outline-edited.pdf`, shown by
`pdf-cli outline show`: Part I (open) > Chapter 1 (open) > Sections 1.1, 1.2, 1.3, then
Chapter Two, Chapter 3 (another file), Chapter 5; "Nouveau — 新しい" (page 4); Part II
(open) > Chapter 4; "Приложение" (page 5).

### Review decisions (2026-10-03)

1. **WebView2 growth rule revised** (ADR 0002): at most 30 MB more for the 1,000-page
   sample than for a one-page document, and memory after three rounds of scrolling within
   10% of one round. Both hold.
2. **`29-slow-first-page`:** accepted as content cost; decoding large JPEG 2000 images at
   the shown resolution is on the Phase 6 checklist (AGENTS.md).
3. **CI:** the Phase 2 commits are pushed.
4. **Byte-for-byte: option (a) accepted.** Bookmarks no edit needs to change keep their
   bytes exactly; neighbours whose links change are rewritten by MuPDF with the same
   destination and action values in MuPDF's spelling. No own serializer for outline items.
5. **Acrobat:** checked by the user. The tree, titles and targets are correct in both
   files. "Chapter 4" shows a JavaScript alert ("1") in both: that is the fixture's own
   JavaScript action (`app.alert(1)`, standing in for another app's script bookmark),
   kept unchanged by the edit, as it should be.

### Behavior choices made without explicit guidance

- A click selects a bookmark and follows it; arrow keys only move the selection; Enter
  follows. The selection stays when you click the page, so repeated Ctrl+B builds a list
  in order.
- The inspector opens from the context menu (Properties) and then follows the selection
  until closed. It floats over the page, so it never changes the zoom of a fit-width view.
- Ctrl+B without selected text titles the bookmark "Page <label>" and opens it for
  renaming. Selected text is used only if the document allows copying.
- A new or retargeted bookmark points at the top-left of what is visible on the page at
  the top of the view (in the page's own orientation), rounded to whole points.
- Deleting a bookmark without children needs no confirmation (it can be undone).
- Expanded states are not written for a signed document without other changes, or when
  permissions forbid outline changes.
- Web links, other files and actions are described in a notice when clicked, never opened
  or run. A bookmark's own color is shown as a dot, not as text color, to keep contrast.
- The sidebar is now 240 px wide (was 192) with Pages and Bookmarks tabs; Pages is the
  default.

### Not verified

- Acrobat (above). A keyboard-only and screen-reader pass of the tree: ARIA roles and
  levels are in place; the full accessibility pass is in Phase 6.
- Dark mode (Phase 5). Fonts on macOS and Linux: no font source yet (ADR 0005, Phase 6).

### CI (after the review)

The first CI runs on these commits failed; each fix is its own commit:

1. **Interop harness:** it printed a CJK bookmark title to CI's cp1252 console and
   crashed. It now writes UTF-8 itself.
2. **No WebDriver session on CI:** the runner's WebView2 (153, Windows Server 2025)
   ignored `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS` because Tauri passes its own default
   arguments through the API, so msedgedriver's `--remote-debugging-port` never arrived.
   Lectrix now creates its main window in code and, when the variable is set, passes both
   argument lists merged. Without the variable nothing changes.
3. **Intermittent empty window:** msedgedriver navigates the webview to `about:blank`
   when a session starts. On the slower runner, Lectrix's page sometimes loaded first and was
   wiped. The harness now reloads the app page in that case, and the frontend asks Rust for
   documents it already has open (`list_open_documents`) before opening startup files. That
   also helps users: if the webview reloads (a renderer crash), open documents reappear
   instead of an empty window. A third end-to-end test covers it.

The harness also kills stray `lectrix.exe` (Lectrix is single-instance), fails fast instead of
retrying, logs every msedgedriver session verbosely, and saves a screenshot when a
document does not show; CI uploads these and runs `tests/e2e/diagnose.ps1` on failure.

## Phase 3: Page labels (report, 2026-10-03)

**Status: complete.** Reviewed by the user on 2026-10-03 (decisions below); Acrobat
checked by the user; CI green on the pushed commits (run 37140827895, including the new
E2E flow and interop `phase3`). The original report follows.

*Report as written:* ready for review, not yet complete. Labels work in the app, and every
local check is green: the Rust tests, the end-to-end tests, all three interop suites and
the local corpus. Two things are still open: CI has not run on these commits (they are not
pushed), and Acrobat has to be checked by you (files below).

### Checklist

| Item | Result |
| --- | --- |
| Everything in 6.3 works, with undo/redo | Yes. A Labels panel lists the rules (one row per range: its pages and first and last label) with an editor for the selected rule: start page, style (none, 1 2 3, i ii iii, I II III, a b c, A B C), prefix, first number. The first rule always starts at page 1 and can't be deleted. Live preview: while you type, the page box, the status bar ("Ch-1 (4 of 8)"), the pages' accessible names and the rule list show the result; Enter, leaving the field or picking a style applies it, and Esc cancels it. Presets: "Roman front matter, then 1, 2, 3 from this page" and "Remove all labels". "New label range from this page" in the panel, the Document menu and the thumbnails' context menu. Letters go a…z, aa…zz, aaa…. Each change is one undo step ("Undo change page labels", "Undo remove page labels"). Existing labels are shown as stored, and saving writes a `/PageLabels` number tree in the catalog. |
| Labels written by the app show correctly in Acrobat's page box and thumbnails | Edge (PDFium) and Firefox (pdf.js): yes, `python tests/interop/run.py phase3`, 11 checks. **Acrobat: needs you** (files below). |
| Existing corpus labels round-trip unchanged without label edits | Yes. On the 13 local-corpus files with labels (up to 2,597 rules in one file), our labels match MuPDF's own reading (`fz_page_label`) on every page. An unrelated edit and an incremental save leave the label tree untouched: none of its objects is written again, so it keeps its bytes. Leaving a field without changing it, or committing the rules as they are, writes nothing and adds no undo step. |

### What was built

- **pdf-core.** `Operation::SetPageLabels { rules }` (an empty list removes the labels),
  checked before anything changes: rules must fall within the document, each start page
  once, numbers from 1 up to the PDF integer limit. `labels::set_rules` writes nothing when
  the rules are already stored, and so creates no undo step (MuPDF drops empty journal
  steps, checked in its source). An edit rewrites the existing tree object in place as one
  flat `/Nums` array, so the catalog keeps its bytes. Sessions report the stored rules
  next to the per-page labels. Guard for damaged files: roman numerals and letters longer
  than 256 characters fall back to decimal, and labels are cut at 256 characters. Without
  it, a file asking for `/St 2147483647` in roman numerals would need gigabytes. Other
  readers have no such limit, so they only differ from us on such files.
- **App.** Rules over IPC (`LabelRule`, `LabelStyle`, generated by ts-rs), the Labels
  panel (`src/lib/features/labels/`), View > Page labels, Document > New label range from
  this page. The panel formats the preview itself, mirroring the Rust formatting with the
  same tests; once a change is applied, the labels Rust returns replace the preview.
  Label actions run one after another, so a field applied on blur and the click that
  caused it (Delete range, a preset) don't overwrite each other. Disabled buttons now look
  disabled everywhere (they didn't before).
- **pdf-cli.** `labels edit` applies rules through a session, as the app does.
- **CI.** The interop step runs `phase3`. It also failed only on the step's last command
  before (a GitHub Actions `pwsh` default), so a `phase0` failure could have passed
  unnoticed. Every command now fails the step.
- No new dependencies.

### Tests

- `cargo test --workspace`: 134 (Phase 2: 124), plus the ignored local-corpus test. New:
  the length guard, `set_rules` writing only changes and reusing the tree object, label
  steps with undo/redo and save/reopen, invalid rules changing nothing, and a hand-written
  fixture with labels "another app" wrote (a `/Kids` tree with `/Limits`, `/Type`, a
  UTF-16 prefix, `/St`). That fixture is read as stored and matches MuPDF. Its tree keeps
  its bytes through a rotation, a new bookmark and an unchanged commit. An edit to it
  writes only the tree object, can be undone, and reads back as written in MuPDF.
- Local corpus (`--ignored`): all 30 files pass. For the 13 that have labels, a label
  edit round trip passes too: a new range at the last page, saved
  incrementally, reads back as written in Lectrix and MuPDF, and `qpdf --check` is no worse.
- Vitest: 45 (Phase 2: 35): formatting with the Rust test values, rules as shown (sorted,
  past-the-end rules hidden, first-page rule added), validation, presets, row text.
- E2E: 4 flows (Phase 2: 3). The new one applies the preset, types a prefix and sees it in
  the status bar and the list before applying it, cancels and applies it, adds a lettered
  range, undoes and redoes, saves, checks the rules on disk with pdf-cli, reopens, and
  jumps by typing "Ch-2" in the page box.
- Interop: phase0 78, phase2 15, phase3 11, all passed. Phase 3 covers letters past Z with
  a prefix ("Anhang Y" to "Anhang FF"), a prefix-only non-ASCII label ("Índice"), the
  other-app fixture before and after an edit, and removed labels. Expected labels are
  written out in the harness, not computed with Lectrix's code. `cargo clippy -D warnings`,
  `cargo fmt`, `svelte-check`: clean.

### Performance (section 2 targets)

Same machine and file as before (`measure.ps1 -Pdf big1000.pdf`, 3 runs): window visible
56 to 112 ms, first page visible 83 to 106 ms, `lectrix.exe` 63 MB idle, whole tree 614 to
627 MB. No change from Phase 2. On the file with 2,597 label rules, the panel shows in
70 ms (24 rows in the page), and the preview follows a keystroke in 63 ms (WebDriver
round trip included).

### Please check in Acrobat

`target/test-output/manual/phase3/`. In each file, type a label into Acrobat's page box
(for example "iv" or "Anhang AA") and look at the thumbnail captions.

| File | Expected labels |
| --- | --- |
| `app-labels.pdf` (30 pages) | i–iv, 1–15, Anhang Y, Anhang Z, Anhang AA, Anhang BB … Anhang FF, Índice ×3 |
| `app-made-in-lectrix.pdf` (8 pages, made in the app by the E2E test) | i, ii, iii, Ch-1, Ch-2, Ch-3, A, B |
| `other-app-labels.pdf` (12 pages, as another app wrote it) | Cover, i, ii, iii, 1–5, Äh-C, Äh-D, Index 120 |
| `other-app-labels-edited.pdf` (the same, decimal range moved to page 6) | Cover, i, ii, iii, iv, 1–4, Äh-C, Äh-D, Index 120 |
| `labels-removed.pdf` | no labels: 1, 2, 3 … |

The two "other app" files have blank pages; only their labels matter.

### Decisions needed

1. **Push to GitHub** so CI runs on these commits (it now also runs interop `phase3` and
   the new E2E flow)?
2. **Thumbnail captions during typing.** The Labels and Pages panels share the sidebar, so
   the thumbnails are not on screen while you type in a label field. While you type, the
   page box, the status bar and the rule list show the new labels; thumbnail captions
   change as soon as the edit is applied (Enter or leaving the field), before saving. Is
   that enough for "live preview", or should the Labels panel show small thumbnails too?
   **Recommendation: keep it as it is.**
3. **Sidebar tabs for Phase 5 (heads-up).** Three tabs fit the 240 px sidebar only as text,
   so their icons are gone. A fourth tab (Annotations) won't fit as text either. Options
   for Phase 5: icon-only tabs with tooltips (as Acrobat's left rail), or a wider sidebar.
   **Recommendation: icon-only tabs**, decided when Phase 5 starts.

### Behavior choices made without explicit guidance

- A new range starts as "1, 2, 3 from 1", like Acrobat's "Begin new section".
- "Roman front matter, then 1, 2, 3 from this page" replaces every rule (also later ones,
  such as an appendix range); it is one undo step. It is disabled on page 1.
- Deleting a range adds its pages to the range before it. The first range can't be
  deleted; "Remove all labels" removes everything.
- Clicking a range selects it and shows its first page. Without a selection, the editor
  shows the range of the current page; focusing a field pins it.
- An invalid value shows why next to the fields ("Another rule already starts at page 5")
  and is not applied; leaving the field puts the stored value back.
- Stored rules that start after the last page (written by other apps) have no effect. They
  are not listed, a note says so, and the next real change removes them. A file without
  a rule at page 1 is shown with "1, 2, 3 from page 1" there, which is how readers number
  those pages anyway.
- A document without labels does not get a `/PageLabels` tree until you change something:
  leaving the default rule as it is writes nothing.
- Changing labels needs the same permission as rotating pages (modify or assemble). In a
  signed document, the first label change shows the usual signed-document warning.

### Not verified

- Acrobat (above). Dark mode (Phase 5). A keyboard-only and screen-reader pass: the list
  is an ARIA listbox and every field has a label; the full accessibility pass is in Phase 6.

### Review decisions (2026-10-03)

1. **CI:** the Phase 3 commits are pushed.
2. **Thumbnail captions during typing:** kept as it is for now (captions change when an
   edit is applied). The panel will probably be redesigned later.
3. **Sidebar tabs:** icon-only tabs with tooltips when Phase 5 adds the Annotations tab.
4. **Acrobat:** checked by the user; the labels show correctly.

## Phase 4: Stitching (report, 2026-10-03)

**Status: complete.** Reviewed by the user on 2026-10-03 (decisions below); Acrobat
checked by the user; CI green on the pushed commits (runs 37154785458 and 37154974372,
including interop `phase4` and the new E2E flows). The original report follows.

*Report as written:* ready for review, not yet complete. Combining files and inserting
pages work in the app, and every local check is green: the Rust tests, the end-to-end
tests (now 7 flows), all interop suites and the local corpus. Two things are still open:
CI has not run on these commits (they are not pushed), and Acrobat has to be checked by
you (files below). One question about signed files is under "Decisions needed".

### Checklist

| Item | Result |
| --- | --- |
| Everything in 6.4 works | Yes. **Combine files** (File menu or the start screen) opens as a tab of its own: the files on the left, every page of every file in one grid. Add files with the dialog or by dropping them on the view. Select pages (click, Ctrl, Shift, arrow keys), reorder them by dragging or with Alt+Shift+arrows, turn them (R, Shift+R) or remove them (Delete), with undo and redo. Bookmarks: one bookmark per file with its bookmarks inside (default), as they are, or none. Labels: each page keeps its label (default), 1, 2, 3 throughout, or none. Combine asks for a new file name (never one of the sources), shows progress with a Stop button, opens the result, and says how many link targets, form fields and attached files were renamed ("src2_…") and which links or bookmarks to left-out pages were removed. Annotations, links, named destinations, form fields, layers and attached files come along with their pages. **Insert pages from file** (Document menu, thumbnails' context menu) uses the same engine: which pages, before or after which page, bookmarks and labels. It is one undo step ("Insert pages"). |
| Three corpus files (with an outline, with labels, with annotations): `qpdf --check` passes, annotations visible in all three engines, internal links work | Yes, for generated files in CI and for real files locally. **CI** (`run.py phase4`, 51 checks): a file with a 3-level outline, a file with labels, and a file with a highlight and links (explicit, named, web), combined whole, combined picked (reordered, turned, a link target left out) and inserted. qpdf is clean; PDFium and pdf.js read the expected pages, labels, bookmarks and link targets; the highlight is visible in MuPDF, PDFium and pdf.js and the engines agree. **Local corpus** (`run.py phase4-local`, 206 checks): `08-calibre-made` (outline, links), `24-rotated-some` (labels, 64 bookmarks, 575 internal links, rotated pages) and `03-annot-highlight` (28 highlights made by another app). qpdf is clean, although one source has qpdf warnings. In PDFium and pdf.js every page keeps its label, each file's bookmarks are nested with their targets moved along, and all 775 links (582 internal) lead where they led in the sources. All 28 highlights are visible in the three engines. |
| Two 500-page files: progress bar, can be cancelled without partial files | Yes, end-to-end test `combine-progress`: two generated 500-page files combine into one 1,000-page file while the progress bar moves (70 values, then "Writing the file…"). A second combine, stopped halfway, writes nothing, and an existing file with the target's name keeps its contents. No temporary file is left. Rust tests cover stopping while copying and while writing. Unslowed, these two files combine in well under a second, too fast to press Stop, so the test slows copying by 3 ms per page (`LECTRIX_COMBINE_PAGE_DELAY_MS`). |

### What was built

- **pdf-core.** A new engine in `merge/`. MuPDF's page graft (used in Phase 0) copies a
  page's contents and resources but leaves out its annotations, so links, notes and form
  widgets were lost. Lectrix now copies pages with its own copier (`merge/copy.rs`), which
  maps every picked page to its new page first and never copies the catalog, the page
  tree or pages that were not picked. Annotations keep their `/P`, `/Popup` and `/IRT`
  links. Links and bookmarks to pages left out are removed (a bookmark with children stays
  as a heading). Named destinations come along; a name already taken is renamed with the
  source's prefix, and the links, bookmarks and actions that use it follow. Top-level
  form fields are renamed the same way, widgets left behind are pruned, and `/DR` and
  `/DA` are merged. Layers (`/OCProperties`) and attached files (`/EmbeddedFiles`) come
  along. Labels: every page keeps the label it had, written as the fewest rules.
  `Operation::InsertPages` runs the same engine inside one undo step. Details, and two
  pitfalls of the `mupdf` crate found on the way, are in ADR 0001.
- **App.** Sources are opened in the registry next to tabs (own sessions, so their
  thumbnails render through the page protocol), but they are never tabs, never recent
  files and not watched. `execute_merge` runs on its own thread with progress on a Tauri
  channel and a cancel flag checked before every page and just before the new file
  replaces anything. `plan_merge` checks the sources first (moved files, tabs with unsaved
  changes). Drops go to the Combine view while it is open.
- **Frontend** (`src/lib/features/merge/`). The Combine view (virtualized grid, keyboard
  and pointer handling, its own undo history), the progress dialog, and the Insert pages
  dialog. A document's page count can now change, so the doc store handles that.
- **pdf-cli.** `merge --pages 2:1-3,1:5@90` (pick, reorder, turn) prints what was renamed or
  left out. `insert` inserts pages through a session, as the app does.
- **Automation.** `LECTRIX_DIALOG` answers the app's file dialogs from the environment (like
  `LECTRIX_OPEN`; paths never come from the webview), and `LECTRIX_COMBINE_PAGE_DELAY_MS` slows
  combining for the progress test. Both are for tests only.
- No new dependencies.

### Finding: big undo steps were slow, now fixed

Inserting a whole 2,881-page book into an open document first took **197 seconds**
(combining the same book into a new file took half a second). The cause is in MuPDF's
undo journal: it records the first change to each object in an undo step by scanning
everything the step has already recorded, and `pdf_insert_page` adds a nested step per
page whose merge is quadratic as well. Now the new pages join the page tree as one new
`/Pages` node (the existing tree changes in two places), the data of objects created in
the step is written with the journal set aside (undo removes those objects whole, so
nothing is lost; a small FFI wrapper with a test), and new bookmarks are written whole.
Lectrix's own name-tree lookup also searched leaves one name at a time, which made reading
a combined file's bookmarks slow; it now searches by halves.

| `pdf-cli insert` into a 3-page document: open, insert as one undo step, save | Before | After |
| --- | --- | --- |
| `18-most-pages` (2,881 pages) | 197 s | 0.4 s (undo 1 ms, redo 9 ms) |
| `16-most-bookmarks` (2,597 pages, 5,023 bookmarks, 18,290 links) | not measured | 2.2 s (redo 58 ms) |
| `12-forms-many-widgets` (343 pages) | not measured | 0.5 s |

### Tests

- `cargo test --workspace`: 164 (Phase 3: 134), plus 2 ignored local-corpus tests. New:
  the copier (shared objects copied once, dropped objects skipped); whole-file combining
  with every reference checked (`/P`, popups, replies, explicit, named and renamed link
  targets, form fields, layers, nested bookmarks); picked pages (reordered, turned,
  inherited boxes and resources written out, nothing of the pages left behind copied);
  options; cancelling and invalid picks; attached files; inserting with undo, redo, save
  and labels; inserting into a nested page tree whose upper nodes turn and crop pages;
  label planning; the journal-free stream wrapper with undo and redo; the checked save;
  sources in the registry (also encrypted ones); the merge runner's progress and
  cancelling.
- Local corpus (`--ignored`): the Phase 1–3 round trip still passes on all 30 files.
  New: each file, combined with a generated file, keeps every page, annotation, label and
  bookmark, and qpdf finds nothing new. All 30 pass.
- Vitest: 54 (Phase 3: 45): moving, nudging, turning and removing pages, range selection,
  drop gaps in the grid, undo history, messages.
- E2E: 7 flows (Phase 3: 4). New: combine three files after removing, turning and moving
  pages, with undo and redo, checked on disk with pdf-cli; insert pages, undo, redo and
  save; the 500-page progress and Stop flow above.
- Interop: phase0 78, phase2 15, phase3 11, phase4 51, phase4-local 206, all passed. The
  PDFium and pdf.js tools now report links and their targets. `cargo clippy -D warnings`,
  `cargo fmt`, `svelte-check`: clean.

### Performance

Section 2 targets (`measure.ps1 -Pdf big1000.pdf`, 3 runs): window visible 60 to 140 ms,
first page visible 104 to 105 ms, `lectrix.exe` 63 to 64 MB idle, whole tree 627 to 647 MB.
No change from Phase 3.

Combining (`pdf-cli merge`, release build; time from start to the file written):

| Files | Result | Time | Peak memory |
| --- | --- | --- | --- |
| Two generated 500-page files | 1,000 pages, 5 MB | 0.16 s | 16 MB |
| `21-rotated-180` + `27-scanned-annotated` | 1,394 pages, 49 MB | 0.36 s | 67 MB |
| `18-most-pages` + `28-scanned-large` | 3,901 pages, 39 MB | 0.42 s | 64 MB |
| `16-most-bookmarks` + `03-annot-highlight` (5,023 bookmarks, 18,290 links) | 2,599 pages, 37 MB | 2.0 s | 144 MB |
| `15-largest-file` + `20-repaired` (two scans, 338 MB) | 684 pages, 323 MB | 1.0 s | 337 MB |

Peak memory is about the size of the result: copied page data stays in memory until the
file is written. Combining two very large scans therefore briefly raises `lectrix.exe` by
that much.

### Please check in Acrobat

`target/test-output/manual/phase4/`. In each file: open the Bookmarks panel and click a
few bookmarks, click the links, type a label into the page box, and look at the
annotations and form fields.

| File | What to see |
| --- | --- |
| `app-combined.pdf` (8 pages, made in the app by the E2E test) | Page 1 has a yellow highlight; page 2 is turned 90°. Labels 1, 1, 2, 3, ii, iii, iv, 2. Bookmarks: three "Lectrix sample" items (pages 1, 2, 5), the second with "Chapter A" (page 3). |
| `merged.pdf` (14 pages) | Labels 1–6, i, ii, 1–3, 1–3. Bookmarks "Outline sample" (Front > Preface, Chapter > Section), "Labels sample", "annotated". On page 12: a highlight with a note, and three links: to page 14, to page 13 (through the named destination "chap2"), and a web link. |
| `picked.pdf` (5 pages) | Page 1 is turned and keeps its highlight. Its link to the left-out page was removed; its other page link leads to page 2. "Front" is a heading without a target, with "Preface" below it. |
| `inserted.pdf` (8 pages) | Pages 3–5 were inserted from the annotated file: labels i–v, then 1–3; its links lead to pages 4 and 5. |
| `names-and-fields.pdf` (7 pages) | Text fields "Name" (value "Ada"), "src2_Name" and "Shared" (two widgets, one value: typing in one fills the other). The link on page 5 leads to page 6 through the renamed target "src2_chap2". The sticky note on page 2 opens its popup; the note on page 4 is a reply to it. The layer "Layer A" is listed and hidden. |
| `attachments.pdf` | Two attached files: "notes.txt" ("from alpha") and "src2_notes.txt" ("from beta"). |
| `local-corpus-combined.pdf` (96 pages, from your library) | Three nested bookmark trees, links inside the second file, the 28 highlights on pages 95–96. |

### Decisions needed

1. **Push to GitHub** so CI runs on these commits (it now also runs interop `phase4` and the
   new E2E flows)?
2. **Signed source files.** A signature is only valid in the file it signed. Combining a
   signed file copies its signature fields with their signatures, and readers will report
   those signatures as invalid in the combined file. Options:
   - (a) Keep it as it is.
   - (b) Warn before combining (or inserting from) a signed file: "The signature in X
     will not be valid in the combined file."
   - (c) Copy signature fields without their signatures, so they show as empty
     signature fields.
   - **Recommendation: (b)**, keeping the fields as they are. You learn why the result
     shows a broken signature, and nothing is silently removed.

### Behavior choices made without explicit guidance

- The Combine view is a tab of its own next to the documents. Closing it asks first if
  pages were arranged but not combined. After combining, the result opens in a new tab and
  the Combine tab stays open for another try.
- Combining reads files as they are saved on disk. If one is open in a tab with unsaved
  changes, Lectrix offers "Save and combine", "Use saved version" or "Cancel".
- Files whose security settings forbid copying content are not added (Acrobat also
  refuses page extraction from them), with a plain message. Encrypted files that allow it
  ask for their password; the combined file is not encrypted.
- The same file can be added twice; the second copy's names get its own prefix.
- Renamed names get a prefix by source position when combining ("src2_chap2") and
  "inserted_" when inserting; if that is taken too, a number is added ("src2_2_chap2").
- Top-level bookmarks follow the order of each file's first page in the result. They are
  named after the document title, or the file name without ".pdf" when there is no
  title.
- "Each page keeps its label": pages of a file without labels are numbered 1, 2, 3… from
  that file's first page. When the result would just be 1, 2, 3… throughout, no
  `/PageLabels` is written.
- Insert pages: the default position is after the current page. "Keep their labels"
  only applies when the inserted file has labels; otherwise the inserted pages continue
  the numbering around them. The file's bookmark goes among the top-level bookmarks,
  before the first one that leads past the insertion point. Afterwards the view goes to
  the first inserted page. Inserting shows no progress bar (see the timings above); the
  dialog's button shows "Inserting…" meanwhile.
- Not carried over from sources: the structure tree (tagged PDF; the result is not
  tagged), article threads, and document-level JavaScript, open actions, viewer
  preferences and XMP metadata.
- The combined file is written with a plain full save: copied streams keep their
  compression, and nothing unreferenced is written. "Save As (optimized)" can still
  recompress it.

### Not verified

- Dropping files from Explorer onto the Combine view: Rust routes drops there while the
  view is open, but an OLE drag can't be scripted (as in Phase 1).
- Acrobat (above). Dark mode (Phase 5). A screen-reader pass: the grid is an ARIA listbox
  with multi-select, and every page reads like "3 of 12: report.pdf, page iv, turned 90°";
  the full accessibility pass is in Phase 6.

### Review decisions (2026-10-03)

1. **CI:** the Phase 4 commits are pushed.
2. **Signed source files: option (b).** Before combining files that are signed, or
   inserting pages from one, Lectrix asks: "The signature in X will not be valid in the
   combined file" (or "in this document"), with "Combine anyway" / "Insert anyway" and
   Cancel. The signature fields are still copied as they are. Covered by Vitest and by an
   E2E flow with a signed file (Cancel writes nothing; "Combine anyway" combines).
3. **Acrobat:** checked by the user; the combined and inserted files show correctly.

## Phase 5: Annotations and repair (report, 2026-10-04)

**Status: complete.** Reviewed by the user on 2026-10-04 (decisions below; the repair box
counts as done with the stand-in problem files); Acrobat checked by the user; CI green on
the pushed commits (run 37186592805, including interop `phase5` and the new E2E flows).
The original report follows.

*Report as written:* ready for review, not yet complete. Every annotation type can be created,
edited, moved and deleted in the app, repair works, and every local check is green: the
Rust tests, Vitest, all interop suites (`phase5`: 528 checks) and the end-to-end tests
(now 9 flows). Three things are still open: CI has not run on these commits (they are
not pushed); the repair box needs your problem files, which I don't have yet; and the
manual checklist is yours to do. Four questions are under "Decisions needed".

### Checklist

| Item | Result |
| --- | --- |
| Every type in 6.5 can be created, edited, moved and deleted, with undo/redo | Yes. A floating toolbar holds Select, Highlight, Underline, Strikeout, Squiggly, Note, Pen and Text box (Esc/H/U/N/P/T), six preset colours, a custom picker, opacity, pen width and font size; the last style of each tool is remembered. Text markup is made from the selection on mouse-up (the viewer sends the selected characters, Rust makes the quads from MuPDF's character quads, so turned or vertical text gets quads in its own direction); a selection across pages makes one annotation per page in one undo step. Alt-drag makes an area highlight. Notes are placed with a click and typed in the inspector; text boxes are typed in place (double-click edits one). Select, move and resize (drawings, text boxes, notes) with handles; Delete. Every change is one named undo step ("Move note"). The **Annotations panel** groups by page, filters by type and author, jumps on click, edits note text, deletes, shows replies under their parent and "needs repair" badges. The **inspector** shows colour, opacity, width or size, note text, author, dates, replies and problems. Covered by Rust tests and the E2E flow "create, edit, delete, undo, save and reopen annotations". |
| Every rule in 5.1 enforced and covered by a round-trip test | Yes. `docs/interop-profile.md` lists, per rule, where it is enforced and which test covers it. The main round trip (`every_type_has_every_profile_key_on_every_page_geometry`) creates all seven types on seven page geometries (normal, rotated 90/180/270, cropped with an offset origin, cropped and rotated, UserUnit 2), saves, reopens and checks every required key and value. Rule 10 has its own test: editing another app's annotation changes only the edited keys, keeps unknown keys, and leaves the other annotations' bytes alone. Two rules needed corrections after MuPDF's appearance synthesis (ADR 0006), and one reading of rule 6 needs your decision (text box colour, below). |
| Interop harness passes for every type on rotated, cropped and normal pages | Yes, `run.py phase5` (528 checks, in CI): all seven types plus an area highlight, on normal, rotated (90, 180, 270), cropped, cropped and rotated, and UserUnit pages, then after edits and a delete. Every annotation is visible in MuPDF, PDFium and pdf.js, the engines agree, and qpdf is clean. The suite renders at 4x: at 2x, the anti-aliased edges of sub-point lines outweighed the line itself in the colour comparison. |
| The repair command fixes the user-supplied problem files so they pass the harness, without changing content, colour, author or position | **Not yet: no problem files from you so far** (section 9 says you'll supply them). In their place: (1) a file with eight annotations written by hand the way broken apps write them (no appearance, quads in each wrong order seen in the wild, a `/Rect` too small, missing `/NM`, `/F`, `/M`, `/P`). Before repair, several are invisible in at least one engine; after repair all are visible in all three, with the same text, colour, author and place (`phase5`, and the E2E flow "repair annotations another app wrote", with undo). (2) The two local-corpus files that need repair, an Acrobat highlight whose `/Rect` misses its quads and a stamp without `/NM` and `/P` (`phase5-local`, 15 checks). |
| The manual checklist in section 9 is prepared | Yes: `docs/manual-checklist.md`, with the files in `target/test-output/manual/phase5/` (about 30 minutes). |
| Settings dialog: author name, appearance System/Light/Dark, applied at once and remembered | Yes. File > Settings (Ctrl+,). The author defaults to the Windows user name (through the platform trait). The appearance applies at once and at startup, stored in app data. A forced theme also retints Mica, which otherwise follows the system theme and left light chrome behind dark text. Covered by the first annotation E2E flow: it sets the author and Dark, checks that the dark theme applies at once, and finds the author in the saved file. |

### What was built

- **pdf-core** (`annot/`). `create` writes the seven types to the write profile: MuPDF's
  appearance synthesis, then `/Rect` with content, stroke and the 1 pt margin, ink
  simplified with Ramer–Douglas–Peucker, text boxes sized to their text with MuPDF's
  Helvetica widths. `edit` changes only the edited keys plus `/M` and the appearance;
  `delete` also removes the popup and replies. `read` lists annotations from the page
  dictionaries without loading pages (1,316 notes in 29 ms) with the problems repair
  would fix; `repair` fixes them. Operations `AddAnnotation(s)`, `UpdateAnnotation`,
  `DeleteAnnotation`, `RepairAnnotations`; the annotate permission is respected.
  `ffi::request_appearance` (`pdf_dirty_annot`) makes repair write a real appearance
  instead of MuPDF's display-only one.
- **App.** Annotation payloads in `DocumentInfo`, and the changed pages' annotations in
  every change and save result (an optimized save renumbers objects, so the ids are sent
  again). `scan_annotations_for_repair` and `repair_annotations`, which logs every change
  with the object number. Settings storage, `get_settings` / `set_settings`.
- **Frontend** (`src/lib/features/annotations/`): toolbar, page interactions, panel,
  inspector, repair dialog; Settings dialog; sidebar tabs are now icons with tooltips and
  accessible names (Phase 3 review).
- **pdf-cli:** `annot note / ink / text / list / edit / delete / repair` and
  `markup --rect`, all through a session as the app does.
- No new dependencies.

### Findings

- **MuPDF's appearances needed small corrections** (ADR 0006, proposed): sticky notes are
  drawn at the size of `/Rect` so every reader shows them the same size; on rotated pages
  PDFium and pdf.js still turn the icon with the page, which no file can change. MuPDF
  writes a `/CL` callout line on every new text box and no margin; it drops `/CA` at
  opacity 1; it gives notes `/F 28`. Lectrix corrects each after synthesis. No type has a
  drawing of Lectrix's own.
- **The app crate's test binary stopped starting** (`STATUS_ENTRYPOINT_NOT_FOUND`). With
  the Phase 5 code, the linker keeps message-box code in the test binary (test binaries
  from earlier phases lack it), which imports `TaskDialogIndirect`, a function only Common
  Controls v6 has; only the app binary carried the manifest asking for v6. `src-tauri/build.rs` now has the linker write the
  same dependency into every binary of the crate (in place of tauri-build's copy, which
  held nothing else; two would clash). `lectrix.exe`'s manifest is unchanged apart from the
  linker's standard "run as invoker" entry.

### Tests

- `cargo test --workspace`: 200 (Phase 4: 164), plus 2 ignored local-corpus tests. New:
  every type on every page geometry with every profile key; edits that change only their
  keys; moving notes, resizing drawings, refitting text boxes; edits a type does not
  support are refused; deleting removes popups and replies only; repair of other apps'
  problems without changing content; annotations and repair as undo steps through the
  session; selected text to quads in the text's direction, and across pages in one undo
  step; quads, ink simplification and text box measuring unit tests; the app's
  annotation payloads.
- Vitest: 64 (Phase 4: 54): tools, page geometry, annotation actions.
- E2E: 9 flows (Phase 4: 7), plus the page-reload check, all passed. New: create, edit,
  delete, undo, save and reopen annotations (with the Settings author); repair annotations
  another app wrote, with undo. These two first waited for the status bar to stop saying
  "Unsaved", which "Saving…" already does, so the app was closed mid-save and the check
  of the file on disk failed; they now wait for "All changes saved", as the other flows do.
- Interop: phase0 78, phase2 15, phase3 11, phase4 51, phase5 528, phase5-local 15, all
  passed. `cargo clippy -D warnings`, `cargo fmt`, `svelte-check`: clean.

### Performance

Section 2 targets (`measure.ps1 -Pdf big1000.pdf`, 3 runs): window visible 29 to 162 ms,
first page visible 86 to 113 ms, `lectrix.exe` 64 MB idle, whole tree 638 to 656 MB.
No change from Phase 4 beyond run-to-run spread.

### Please check

The manual checklist, `docs/manual-checklist.md`: Acrobat Reader, Edge, Firefox and Foxit,
with the files in `target/test-output/manual/phase5/`.

### Decisions needed

1. **Push to GitHub** so CI runs on these commits (it now also runs interop `phase5` and
   the two new E2E flows)?
2. **Problem files.** Please put the PDFs whose annotations display wrongly in Acrobat
   somewhere I can read them (they can stay out of git, like the local corpus). I'll add
   them to the harness as a local suite and report what repair does to each.
3. **Text box colour (rule 6).** Rule 6 lists `/C` (colour) for every annotation. For a
   text box, though, `/C` is the background fill (PDF 32000-1 12.5.6.6); the text colour
   lives in `/DA`. Options:
   - (a) Write `/C []` (no background) and the chosen colour as the text colour in `/DA`.
     This is what Lectrix does now, and what the spec describes.
   - (b) Write the chosen colour in `/C` too. Readers would then fill the box with the
     text's colour, making the text unreadable.
   - (c) Offer a background colour for text boxes as well (`/C` = background, `/DA` =
     text): a second colour control in the inspector.
   - **Recommendation: (a)**, with rule 6 read as "`/C` as the type defines it". (c) can
     come later if you want filled boxes.
4. **ADR 0006** (the corrections above) is "proposed": approve it, or tell me what to
   change.

### Behavior choices made without explicit guidance

- Notes are placed with one click and their text is typed in the inspector, which opens
  focused on the note field. Text boxes are typed in place where you click; Ctrl+Enter or
  a click outside finishes, Esc discards. An empty new box is not created; emptying an
  existing one deletes it (one undo step).
- The inspector's colour for a text box is the text colour (see decision 3).
- Deleting an annotation that has replies asks first, then deletes the replies and the
  popup with it.
- A text box from another app that has a callout line can be edited but not moved or
  resized, because its line would need moving too.
- The Annotations panel lists markup, notes, drawings, text boxes, shapes, stamps and
  attachments; popups, links and form widgets are not listed.
- Repair counts `/Rect` as too small only past the 1 pt margin: Acrobat's own highlights
  miss skewed quads by a fraction of a point. Unreadable QuadPoints are reported and left
  alone.
- Settings is under the File menu (Ctrl+,), as in most Windows apps.

### Not verified

- Acrobat, Foxit, Edge and Firefox by a person (the manual checklist).
- Touch and stylus input: the pen tool was tested with scripted mouse input (E2E) only.

### Review decisions (2026-10-04)

1. **CI:** the Phase 5 commits are pushed.
2. **Problem files:** you have none of your own, so the repair box counts as done with
   the stand-ins: the hand-written problem file (every problem section 5.3 lists) and the
   two local-corpus files that need repair. Each is invisible in at least one engine
   before repair and visible in all three after, unchanged in content, colour, author and
   place.
3. **Text box colour: option (a).** `/C []` (no background), the colour in `/DA`; rule 6
   reads "`/C` as the type defines it" (`docs/interop-profile.md`).
4. **ADR 0006:** accepted.
5. **Acrobat:** checked by you; everything works. In `types-rot90.pdf`, the note, drawing,
   text box and area highlight neither follow the text nor turn with the page, unlike in
   `types-normal.pdf`. That is how the file was made: those four were placed at the same
   spot on screen as in the normal file, upright as seen, which is what the app does when
   you annotate a turned page (the text box carries `/Rotate 90`, as Acrobat writes, so
   Acrobat keeps it upright when you edit it). The text markups follow the text. The
   checklist now says so.

## Phase 6: Polish and release (report, 2026-10-04)

**Status: ready for review, not yet complete.** Crash recovery, the installers with the PDF
choice, the accessibility pass and the macOS/Linux CI jobs are built, and every local check
is green: the Rust tests, Vitest, all interop suites, the local corpus and the end-to-end
tests (now 16 flows). Three things are still open:

- CI has not run on these commits (they are not pushed), so the new macOS and Linux jobs
  and the installer check have never run.
- One performance gap remains, for scanned books stored as JPEG 2000. Closing it needs a
  decision about MuPDF.
- The manual checklist (installers, a real crash, Narrator) is yours to do.

### Checklist

| Item | Result |
| --- | --- |
| Crash recovery, recent files and per-file view memory work | Yes. **Crash recovery:** every 2 minutes, each document with unsaved changes gets a recovery copy in `%LOCALAPPDATA%\org.lectrix.pdf\recovery` (only when its content changed since the last copy). Saving, reloading or closing a document deletes its copy, and so does quitting normally (after "Save" or "Don't save"). At the next start, copies a crash left behind are offered: "Restore unsaved changes?" with the file names and times, and Restore, Discard… (asks once more) or Not now (asked again next time). A restored document opens in a tab for its own file, marked unsaved. Save writes onto the file incrementally, so its original bytes stay intact (section 5.4). If another program changed the file since the copy was made, the usual "changed on disk" banner appears. Encrypted files ask for their password again. **Recent files and remembered views** (built in Phase 1) now have an end-to-end test: a closed document reopens from the start screen's list on the same page and zoom. |
| NSIS and MSI installers build in CI; `.pdf` association can be chosen at install | Built locally; the CI step is written but has not run. Both installers have a "PDF files" page after the folder page: "Open PDF files with Lectrix", checked by default. When it is chosen, Lectrix is registered for PDFs: it appears in Open with and in Settings > Default apps, and Windows asks which app to use at the next PDF. Windows 10 and 11 let no installer take over the default itself (ADR 0007, proposed). For silent installs, `/NOPDF` (NSIS) or `LECTRIX_ASSOCIATE_PDF=0` (MSI) leaves the registration out. `tests/installer/check.ps1` installs and uninstalls both silently, with and without the registration, checks the registry and files, and checks that the `.pdf` default is never touched. It installs software, so it runs on CI runners only; I have not run it on your machine. I checked the built MSI's tables instead (feature, condition, registry rows, the dialog's events). Both installers now also install `LICENSE.txt` and the license notices. |
| Performance targets met, or gaps documented | Met on the generated 1,000-page file and on most real files (table below). One gap: scanned books stored as JPEG 2000 (decision 2). |
| Large JPEG 2000 pages: look into decoding at the resolution shown, and report | Done (section below). Partly fixed in Lectrix: that page now shows in 1.05 s instead of 1.85 s. Decoding at the shown resolution needs a change inside MuPDF: decision 2. |
| Accessibility pass (keyboard-only walkthrough, contrast check) | Done, with fixes (below). An end-to-end test walks every Tab stop in every panel and dialog, and a keyboard-only session reaches a saved file. Contrast is checked by a test against `app.css` in both themes and for all 48 Windows accent colours. |
| macOS and Linux build-only CI jobs; platform gaps listed | The jobs are written: they build the workspace, its tests and the app, and run the tests without failing the job. They have not run, because nothing is pushed. WSL here has no compilers and needs a password for `sudo`, so I could not build for Linux locally. Platform gaps are listed below. |

### What was built

- **pdf-core.**
  - `ffi::save_snapshot` wraps MuPDF's `pdf_save_snapshot`: it writes the file plus its
    unsaved changes as an incremental update, without changing the document, its undo or
    its redo history (tested).
  - `Session::write_recovery` writes a copy only for a new unsaved revision. Damaged
    files that MuPDF repaired are copied in full instead.
  - `Session::restore` opens a copy as the document of its original file: it reads from
    the copy, saves to the original, and stays dirty until saved.
- **App.**
  - `recovery.rs` keeps one slot per document: `<slot>.json` names the file, and
    `<slot>-<n>.pdf` is the copy. A new copy gets a new number, and the description is
    replaced only once the copy is complete. A restored document's copy is kept while the
    document reads from it.
  - New commands: `list_recovered`, `restore_recovered`, `discard_recovered`.
  - Recovery is off for test and measurement runs (`LECTRIX_EPHEMERAL`).
    `LECTRIX_RECOVERY_DIR` and `LECTRIX_RECOVERY_INTERVAL_MS` exist for tests only.
- **Installers.** `src-tauri/windows/installer.nsi` is Tauri 2.12.1's NSIS template plus
  the page, with every change marked `Lectrix:`. `src-tauri/windows/pdf-association.wxs` is
  a WiX fragment with the registration as an optional feature and the page as a dialog.
  ADR 0007 explains the approach.
- **Accessibility fixes**, from the walkthrough and the contrast test:
  - The system accent colour is adjusted per theme, so focus rings, selected tabs and
    outlines keep 3:1 against every surface (a gold accent was 1.7:1 on white). Text on
    it is black or white, whichever reads better.
  - Text fields have a 3:1 bottom edge and a 2 px accent underline on focus, as Windows
    11 text boxes do; their light border alone was 1.3:1. Every field now uses the same
    style.
  - Muted text is a shade darker in the light theme: 4.25:1 on a selected row over the
    page background before, 4.6:1 now.
  - Menu items, the label ranges list and the custom colour swatch now show keyboard
    focus.
  - Shift+F10 and the Menu key open the focused control's context menu (thumbnails,
    bookmarks, pages). Some actions, such as a bookmark's Properties, were reachable only
    by right-click.
  - Text fields in panels swallowed every key, so Ctrl+S, Ctrl+F and the other app-wide
    shortcuts did nothing while typing a note or a bookmark title. They now pass those
    through. Saving or closing from a field commits it first, and the store waits for
    edits in flight before saving.
- **Viewer.** Thumbnails wait while pages on screen render (see the JPEG 2000 section).
- **Licenses.** `tests/licenses/notices.py` lists every Rust crate compiled into
  `lectrix.exe` and every npm package bundled into the frontend (260 in all). It checks
  that each license is AGPL-compatible and writes `THIRD_PARTY_LICENSES.md` with their
  texts. CI runs it with `--check`.
- **pdf-cli:** new `recovery` command (write a copy, time it, restore it).
- **CI:** installer check, license check, macOS and Linux jobs.
- No new dependencies.

### Findings

1. **MuPDF cannot load a saved undo history.** MuPDF can save a document's undo history
   next to a snapshot (`pdf_save_journal`). In 1.27.2, loading it back fails as soon as
   the history records any change: `pdf_deserialise_journal` and
   `pdf_add_journal_fragment` disagree about where entries go (details in ADR 0001). So a
   restored document's undo history starts at the restore. If a later MuPDF fixes this,
   restoring the history is a small change. The bug could be reported upstream.
2. **MuPDF decodes JPEG 2000 one image at a time, at full resolution, on one thread.**
   Details in the JPEG 2000 section.
3. **Recovery copies of large files take a moment.** The copy includes the whole file,
   and while it is written, that document renders nothing new. Measured with
   `pdf-cli recovery`:

   | File | Copy | Time |
   | --- | --- | --- |
   | `18-most-pages` (29 MB, 2,881 pages) | snapshot | 0.10 s |
   | `16-most-bookmarks` (34 MB) | snapshot | 0.18 s |
   | `29-slow-first-page` (95 MB) | snapshot | 0.29 s |
   | `20-repaired` (165 MB, damaged) | full copy | 1.2 s |
   | `15-largest-file` (173 MB, damaged) | full copy | 1.5 s |

   A copy is written at most every 2 minutes, and only after a change. Restoring takes
   8 to 309 ms.

### JPEG 2000 pages (Phase 2 finding)

Page 1 of `29-slow-first-page` is a 4975 × 6658 JPEG 2000 image (33 megapixels, a 1.9 MB
stream). The app draws it at 1421 × 1902.

- **MuPDF ignores the resolution asked for.** For JPEG 2000, MuPDF's image code
  (`compressed_image_get_pixmap`) calls `fz_load_jpx`, which always decodes every pixel.
  Other formats get a reduced decode (`l2factor`). OpenJPEG itself can decode at ½, ¼ and
  so on (`cp_reduce`), but MuPDF never asks it to. MuPDF's OpenJPEG is also built without
  threads, so `OPJ_NUM_THREADS` changes nothing (922 to 925 ms).
- **MuPDF decodes one JPEG 2000 image at a time.** Its OpenJPEG glue takes a global lock
  around every decode (`fz_opj_lock`, shared with FreeType). In the app, the thumbnail of
  page 1 often started first and held the lock for its own 0.9 s decode, so the page
  showed after 1.85 s.
- **Fixed in Lectrix:** thumbnails now wait while pages on screen render. The page shows in
  1.05 to 1.07 s. A heavy non-JPEG page elsewhere (`01-annot-caret`) went from 644 to
  445 ms.
- **What decoding at the shown resolution would gain**, measured with OpenJPEG 2.5.4
  (through Pillow) on the same image: full 848 ms, ½ (2488 × 3329) 353 ms, ¼
  (1244 × 1665) 185 ms. This file has three resolution levels, so ¼ is the most. At fit
  width MuPDF would ask for ½, so that page would show in about 0.5 s. Thumbnails could
  use ¼.
- **The bigger cost is scrolling.** The book's other pages are archive.org-style scans:
  1643 × 2200 JPEG 2000 plus a small mask, about 0.2 to 0.4 s each to decode. They are
  close to the size drawn at fit width, so a reduced decode would not help them. What
  would help is decoding several at once. Scrolling this book now, with the same test as
  on the generated file:

  | | Blank stretches | Over 200 ms | Worst |
  | --- | --- | --- | --- |
  | Steady, 2,000 px/s | 46 | 11 | 1,003 ms (the first pages) |
  | Fast, 6,000 px/s | 142 | 79 | 404 ms |
  | Random jumps | 28 | 26 | 413 ms |

  `lectrix.exe` reached 252 MB after this scroll, from decoded images in flight. Idle with
  the book open it uses 101 MB.
- Both remedies, reduced-resolution decoding and multithreaded decoding without the global
  lock, are changes inside MuPDF (`source/fitz/load-jpx.c`, `image.c`, and how
  `mupdf-sys` builds OpenJPEG): decision 2.

### Tests

- `cargo test --workspace`: 216 (Phase 5: 200), plus 2 ignored local-corpus tests, which
  pass on all 30 files. New tests cover:
  - snapshots: they leave the document, undo and redo as they were; a reopened snapshot
    starts with the original bytes; damaged files are refused for snapshots and copied
    in full instead;
  - copies are written only for new unsaved revisions;
  - restored sessions: dirty, save incrementally onto the original (qpdf-clean), write
    copies of their own, reload the original, keep an encrypted file encrypted, and
    notice when their file changed since the copy;
  - recovery slots across a crash, a save, an undo back to saved, a close and a normal
    quit, and leftover or badly named files.
- Vitest: 127 (Phase 5: 64). New: the recovery question's wording; WCAG contrast of every
  token pair in both themes and of all 48 Windows accents; the accent shading; keys in
  panel text fields; context-menu keys; render scheduling.
- E2E: 16 flows (Phase 5: 10), all pass. New:
  - a crash and a restore (Not now, then Restore, then Save, checked on disk);
  - Discard, and a normal quit with "Don't save" leaving no copies;
  - every Tab stop named and showing focus, on the start screen, in every panel, the
    inspector, search and Settings;
  - arrow keys in menus, tabs and lists, keyboard context menus, focus kept inside
    Settings;
  - a keyboard-only session: go to a page, add and rename a bookmark, edit a note, save
    from the note field, checked on disk;
  - recent files and the remembered view.
- Interop: phase0 78, phase2 15, phase3 11, phase4 51, phase5 528, phase4-local 206,
  phase5-local 15, all passed. License check: 260 packages, all allowed. `cargo clippy -D
  warnings`, `cargo fmt` and `svelte-check` are clean.
- Not run anywhere yet: the installer check and the macOS and Linux jobs (CI).

### Performance (section 2 targets)

Same machine as before, release build, PNG.

| Target | Measured | Met? |
| --- | --- | --- |
| Window visible < 1 s | 30 to 99 ms | Yes |
| First page of a large PDF visible < 1 s | 1,000-page file 73 to 80 ms; 2,881-page dictionary 137 ms; JPEG 2000 scan 1.05 to 1.07 s | Yes, except the JPEG 2000 page (decision 2) |
| No blank page > 200 ms while scrolling | 1,000-page file: steady none, fast worst 18 ms, jumps worst 46 to 54 ms. JPEG 2000 book: see above | Yes, except JPEG 2000 scans |
| `lectrix.exe` < 200 MB idle (ADR 0002) | 63 to 64 MB with the 1,000-page file, 101 MB with the JPEG 2000 book | Yes |
| WebView2 (ADR 0002 rule) | Whole tree 635 to 647 MB idle (Phase 5: 638 to 656 MB). Not re-measured against the one-page baseline, as nothing this phase changes what the webview holds | Unchanged |

### Platform gaps (macOS and Linux)

The code is meant to build on all three platforms: both `platform` modules have
fallbacks, and the file-stream shim has a POSIX branch. None of this has been run yet.

- **Fonts:** there is no installed-font index (ADR 0005). Non-embedded fonts other than
  the base 14 render with MuPDF's substitutes, and non-embedded CJK text has no glyphs.
  This needs fontconfig (Linux) and Core Text (macOS) behind the existing `SystemFonts`
  trait.
- **Window:** no Mica, no accent colour and no theme retint. The transparent, undecorated
  window would need macOS's private API (`macOSPrivateApi`) to be translucent, and the
  custom title bar's window buttons follow Windows (macOS puts them on the left).
- **Platform services:** the default author comes from `USER`. Same-file checks rely on
  `canonicalize`, which is case-sensitive on Linux, as it should be there. WebView2's
  memory target has no equivalent.
- **Installers and file association:** Windows only. `.dmg`, `.deb`/AppImage and their
  associations are not configured. Crash recovery, the single-instance hand-off and
  `LECTRIX_DIALOG` are platform-neutral but untested there.
- **Tests:** some are Windows-only by design (DirectWrite fonts, locked files, the
  `qpdf.exe` lookup). The informational test step will show the rest.

### Accessibility: what remains

- **Creating and moving annotations needs a pointer.** Text markup is made from a mouse
  selection; notes, drawings and text boxes are placed by clicking; moving and resizing
  use handles. Editing, deleting and repairing work from the keyboard (the Annotations
  panel and the inspector). Acrobat's caret browsing (selecting text with the keyboard)
  would be the way to add this; it is a feature of its own.
- **Page text is not exposed to screen readers.** Pages are images, and the text layer is
  used for selection only.
- **Windows high-contrast themes** are not handled specifically. WebView2 applies the
  system colours itself, which I have not checked.
- A screen-reader pass with Narrator is on the manual checklist. The automated walk uses
  Chromium's own accessible names and roles.

### Please check

The manual checklist, in `docs/manual-checklist.md` (new part "installers, crash recovery,
accessibility"): the installers' pages and what Windows does afterwards, a real crash in
Task Manager, and a short Narrator walk. About 20 minutes.

### Decisions needed

1. **Push to GitHub** so CI runs these commits? CI now also runs the installer check, the
   license check and the new macOS and Linux jobs, none of which have run. I expect a
   round or two of fixes on the macOS and Linux jobs.
2. **JPEG 2000 scans (performance gap).** Both remedies are changes inside MuPDF:
   decoding at the resolution shown (about 30 lines in `load-jpx.c` and `image.c`), and
   decoding several images at once (OpenJPEG built with threads and MuPDF's global lock
   lifted, which is a larger change). Options:
   - (a) Carry the reduced-resolution patch in a Lectrix fork of `mupdf-sys` (a git
     dependency, like the vendored `mupdf` crate), and offer it to Artifex upstream.
   - (b) Offer it upstream only, and wait for a MuPDF release.
   - (c) Accept the gap for JPEG 2000 books and record it.
   - **Recommendation: (a), for the reduced-resolution patch only.** It is small and
     contained, and it fixes oversized pages and thumbnails. Lifting the global lock is
     riskier; for that one, (b).
3. **ADR 0007 (installer PDF registration)** is proposed: approve it, or tell me what to
   change. In particular, the box is checked by default.
4. **AGPL source offer (licensing).** The installers now carry the license and the
   notices. AGPL section 6 also asks that people who get the binary can get the source.
   The usual way is a link to the public repository in the installer and in the app (an
   About box, which v1 does not have). Where should it go, and which URL?

### Behavior choices made without explicit guidance

- Recovery copies are written only when the content changed since the last copy. They
  are deleted as soon as nothing is unsaved, including after undoing back to the saved
  state.
- The restore question comes before the files Lectrix was started with, so a recovered file
  opens with its changes rather than without them. Escape means "Not now", never
  "Discard", and discarding asks once more.
- A restored document's tab has its own file's name and is unsaved. If its file was moved
  or deleted meanwhile, it still restores, and Save As puts it elsewhere. If the file is
  already open in a tab, its changes are kept for the next start.
- Undo after a restore starts at the restored state (finding 1).
- Encrypted documents are recovered too: the copy keeps their encryption, and restoring
  asks for the password.
- The installers' PDF box is checked by default, and silent installs register Lectrix
  unless told not to.
- Muted text in the light theme is now `#575757` (was `#5c5c5c`). The system accent may
  be shown a little darker (light theme) or lighter (dark theme) than Windows shows it,
  when that is needed for 3:1 contrast.

### Review decisions (2026-10-04)

1. **CI:** the Phase 6 commits are pushed. The user reports the CI results (including the
   first runs of the macOS and Linux jobs and the installer check) and does the manual
   checklist.
2. **JPEG 2000 scans: option (a).** The reduced-resolution decoding patch is carried in a
   Lectrix fork of `mupdf-sys` and offered upstream; lifting MuPDF's global JPEG 2000 lock
   is left to upstream.
3. **ADR 0007:** accepted, with the PDF box checked by default.
4. **AGPL source offer:** an About dialog in the app with a link to the source.

### After the review (2026-10-04)

- **About dialog (decision 4).** Help > About Lectrix shows the version, the AGPL notice,
  MuPDF's credit, where the license files are installed, and the source code address with
  a Copy button (Lectrix stays offline, so the address is shown, not opened). The address is
  one constant, `SOURCE_URL` in `src/lib/config.ts`: `https://github.com/pan4ratte/lectrix`.
  The repository was made public (and renamed from `sci-pdf`) on 2026-10-04, so the
  address works for everyone. The accessibility E2E flow covers the dialog.
- **JPEG 2000 at the resolution drawn (decision 2, option (a)), ADR 0008.** MuPDF is now
  built from Lectrix's fork of `mupdf-rs` (`pan4ratte/mupdf-rs`, branch
  `folio-mupdf-1.27.2`), which patches MuPDF's JPEG 2000 decoding at build time. The same
  change as a plain diff for Artifex is in `docs/upstream/`, for you to send.
  - `29-slow-first-page`, first page in the app: 1.05 s → **376 to 382 ms** (target met).
  - `pdf-cli`: page 1 at fit width 992 → 425 ms; its thumbnail 904 → 42 ms; body-page
    thumbnails 113 → 47 ms; body pages at fit width unchanged (219 → 218 ms), as they
    already decode at about the size drawn.
  - Scrolling that book was not re-measured: the scroll test is driven by
    `requestAnimationFrame`, which WebView2 stops while the window is covered, and the
    machine was in use. Body pages render as before, and cheaper thumbnails hold MuPDF's
    JPEG 2000 lock for less time, so it should be no worse than the table above; scroll
    blanking on such books remains a known gap until MuPDF decodes several images at once.
  - New test `crates/pdf-core/tests/jpx.rs` (small renders match the full decode scaled
    down; fixture `tests/fixtures/quadrants-1024.jp2`, 14 KB). pdf-core tests (137) and
    the local corpus pass with the fork.
- Two outline tests saved without the guard against `qpdf` runs in parallel tests
  (ADR 0003), and one failed once; both are guarded now.
- The first CI run after this change rebuilds MuPDF from the fork (no cache yet).
