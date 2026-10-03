# Progress reports

## Phase 0: Feasibility spike (report, 2026-10-03)

**Status: complete.** Every box is checked, and CI passed on GitHub (run 37073273603,
`windows-latest`, all steps green, installers uploaded as an artifact). The first run took
79 minutes, mostly compiling MuPDF from source three times (clippy, tests, release build);
later runs reuse the Rust cache. Reviewed by the user on 2026-10-03 (decisions below).

### Checklist

| Item | Result |
| --- | --- |
| Tauri 2 + SvelteKit (static, SSR off) + Tailwind skeleton builds and runs on Windows | Done. Release build `target/release/folio.exe`. |
| MuPDF linked through the `mupdf` crate; version pinned and recorded | Done. MuPDF 1.27.2. The crate is a vendored, patched copy at upstream `537d505`; see ADR 0001 and `docs/versions.md`. |
| A page renders in the window through `folio://`; render and encode times measured on a 500-page file | Done. See the performance table below. |
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
| Idle memory < 200 MB, one large document open | `folio.exe` 60 MB working set (13 MB private). Including the WebView2 processes: about 500 MB | Depends on the definition, see finding 3 |

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
3. **Memory target definition.** Folio's own process uses about 60 MB. WebView2's browser,
   GPU and renderer processes add about 440 MB, and we can only partly control that. Should
   the 200 MB target cover `folio.exe` only, or the whole process tree including WebView2?
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
2. **Memory target: decided in ADR 0002.** 200 MB applies to `folio.exe`; the whole tree
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
  `%LOCALAPPDATA%\org.folio.pdf\logs`, and a `platform` module (Mica, accent color, path
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
  the file open. Folio starts no child processes; details in ADR 0003.

### Performance (section 2 targets)

Machine: this development laptop, Windows 11, 1.5× display scaling, release build. File:
the generated 1,000-page text PDF. Run: `tests/perf/measure.ps1 -Pdf big1000.pdf -Perf
[-Format png]`.

| Target | Measured | Met? |
| --- | --- | --- |
| Window visible < 1 s | 31–195 ms | Yes |
| First page visible < 1 s | 72–160 ms (Phase 0: 1,300–1,607 ms) | Yes |
| No blank page > 200 ms while scrolling (1,000 pages) | Continuous: 0 blank events at 2,000 px/s (46 pages); worst 122 ms at 6,000 px/s (139 pages). Random jumps: worst 200–246 ms (RGBA), 171 ms (PNG) | Yes for scrolling; jumps borderline |
| folio.exe < 200 MB (ADR 0002) | 105–108 MB idle with the document open; 137–141 MB after scrolling 185 pages | Yes |
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
   app still defaults to RGBA as section 3 says; `FOLIO_IMAGE_FORMAT=png` switches it, and
   both paths share one cache.
   - (a) Make PNG the default and record it in an ADR. One-line change.
   - (b) Keep RGBA and move pixels through WebView2's shared-buffer API (no copies).
     Probably the fastest, but Windows-only, needs new `webview2-com` code behind the
     `platform` trait, and would be a project of its own.
   - **Recommendation: (a) now;** revisit (b) only if large scans prove slow.
2. **WebView2 memory breaks ADR 0002's growth rule.** Folio's own process is well under
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

| File | First page visible | folio.exe |
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

**Status: ready for review, not yet complete.** Bookmarks work in the app and every local
check is green, including the new end-to-end tests. Four things stand between this and
"done": CI has not run on these commits (not pushed), Acrobat has to be checked by you,
the WebView2 growth rule is still not met (a revision is proposed), and one real-world file
still takes 1.8 s to show its first page for a reason unrelated to fonts. Decisions 1 to 4
below need you.

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
  operations and `set_bookmark_open` over IPC, types generated by ts-rs. `FOLIO_OPEN`
  opens files like command-line arguments (tauri-driver hands launch arguments to WebView2
  on Windows). Page canvases are CPU-backed and fewer pages are mounted (memory); a
  minimized window sets WebView2's memory target to Low.
- **pdf-cli.** `outline show`, `outline edit --op ...` (through the same session path as
  the app), `fonts` (index, name resolution, a document's non-embedded fonts), rotated
  pages in `info`.
- **Tests and tools.** `tests/e2e/` (standalone webdriverio 9.32.0, Node's test runner,
  `fetch-edgedriver.ps1`), interop `phase2`, `measure.ps1 -Empty`, `FOLIO_PERF=scroll`,
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

Generated 1,000-page file; WebView2 = whole tree minus `folio.exe`.

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
| folio.exe < 200 MB | 63 MB idle (Phase 1: 105 to 108); 104 MB after scrolling | Yes |
| WebView2 growth ≤ 100 MB | about +120 MB | **No**, decision 1 |

### Decisions needed

1. **WebView2 growth rule.** What remains (+120 MB) is the cost of showing one page, and no
   longer depends on the document. Options:
   - (a) Revise ADR 0002's rule to what Folio controls: opening a large document may not
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
