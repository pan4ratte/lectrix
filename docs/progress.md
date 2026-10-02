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
