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

**Status: ready for review, not yet complete.** The viewer, saving and file handling work,
and local checks are green. Three things stand between this and "done": CI has not run on
these commits (they are not pushed yet), WebView2 memory breaks the growth rule in ADR 0002,
and raw RGBA turned out slower than PNG, which conflicts with section 3. Decisions 1 and 2
below need you.

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
