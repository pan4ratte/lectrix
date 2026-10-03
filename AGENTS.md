# PDF Editor — Build Instructions for Coding Agent

Oct 2, 2026 · @Mark

## 1. Mission and scope

Build a lightweight, open-source PDF editor for Windows, designed to go cross-platform later, whose annotations, bookmarks and page labels open correctly in Acrobat and every other mainstream reader. Interoperability is the product's core promise: a feature that works only inside this app is a failed feature.

This document is the single source of truth for the build. Read it in full at the start of every session. The working name is **Folio**; keep it in one config constant so it can be renamed.

**In scope for v1:**

- **Viewer:** fast continuous scrolling, zoom, thumbnails, text selection, text search, tabs for multiple documents.
- **Bookmarks:** create, rename, reorder, nest, delete, and retarget the document outline.
- **Page labels:** assign label ranges (roman front matter, arabic body, prefixed appendices).
- **Stitching:** combine several PDFs into one, at page level, with bookmarks and labels merged sensibly.
- **Annotations:** highlight, underline, strikeout, squiggly, sticky note, freehand ink, text box, plus an annotation list.
- **Repair annotations:** normalize broken annotations written by other apps so they display everywhere.

**Out of scope for v1** (do not build, even partially): form filling, digital signing, OCR, editing page text or images, redaction, converting other formats to PDF, cloud sync, mobile builds.

## 2. Tech stack and constraints

The stack is fixed: Tauri 2, a Rust backend running MuPDF, and a Svelte 5 frontend styled with Tailwind. Use the latest stable release of each at project start, pin exact versions, and record them in `docs/versions.md`.

| Layer | Choice | Notes |
| --- | --- | --- |
| App shell | Tauri 2 | WebView2 on Windows. Bundle with the Tauri bundler (NSIS and MSI). |
| Backend language | Rust, stable toolchain | All PDF logic lives here. |
| PDF engine | MuPDF (C library) | Via the `mupdf` Rust crate; call the C API directly through `mupdf-sys` where the crate lacks coverage. |
| Frontend | Svelte 5 (runes) on SvelteKit | `adapter-static`, SSR disabled, as Tauri's own SvelteKit guide recommends. TypeScript in strict mode. |
| Styling | Tailwind CSS | Through its Vite plugin. Design tokens as CSS variables. |
| UI primitives | A headless Svelte component library (e.g. Bits UI) and Lucide icons | Headless only, so the visual design stays ours. |
| Rust/TS type sharing | `tauri-specta` or `ts-rs` | Generate TypeScript types for every IPC command and payload; never hand-write them. |
| Testing | `cargo test`, Vitest, `tauri-driver` with WebdriverIO | See section 9. |

**Hard constraints:**

- **License:** the whole project is AGPL-3.0-or-later, because MuPDF is AGPL. Every new dependency must be AGPL-compatible (MIT, Apache-2.0, BSD, MPL-2.0 are fine). Maintain `THIRD_PARTY_NOTICES.md`.
- **Platforms:** Windows 10 (1809+) and Windows 11, x64 first. Anything Windows-specific goes in a `platform` module behind a trait, so macOS and Linux can be added without touching feature code.
- **Offline:** no network access at runtime, no telemetry, no update checks in v1.
- **Security:** minimal Tauri capabilities. File access only to paths the user picked through a dialog, drag-and-drop, or file association. Strict CSP, no remote content in the webview.
- **Performance targets** (measure from Phase 0 and report): window visible in under 1 s; first page of a 500-page PDF visible in under 1 s; no blank page visible for more than 200 ms while scrolling; idle memory under 200 MB with one large document open.

## 3. Architecture

All PDF reading and writing happens in Rust through MuPDF; the Svelte frontend never parses or modifies PDF bytes. The frontend displays rendered pages and draws interactive overlays, and every change it makes is a typed operation sent to Rust.

&#91;embedded content: architecture · three layers over MuPDF\]

Down-arrows carry typed operations and calls; up-arrows carry page images, revisions and results.

**Three layers:**

1. **`pdf-core` crate** (plain Rust library, no Tauri dependency). Owns document sessions, rendering, text geometry, the annotation write profile, outlines, page labels, merging, repair, undo/redo and saving. Everything in it must be testable headless.
2. **`src-tauri` app crate.** A thin layer: IPC commands that call `pdf-core`, the page-image protocol, file dialogs, menus, window effects, file association, logging.
3. **SvelteKit frontend.** Views, editors, overlays and UI state only.

**Threading.** MuPDF contexts and documents must not be shared across threads without care. Give each open document its own actor thread that exclusively owns its MuPDF document; commands reach it through a channel and get replies back. For parallel rendering, build a display list per page on the actor thread, then render display lists on a small worker pool with cloned contexts, as MuPDF's multi-threading guide describes. In Phase 0, check whether the Rust crate's types are `Send`/`Sync` and document the result in an ADR.

**Rendering pipeline.**

- The frontend requests pages through a custom URI protocol: `folio://page/{docId}/{pageIndex}?scale={s}&rev={r}`. Including the document revision in the URL makes cache invalidation automatic.
- Rust renders the page with MuPDF and returns an image. Start with PNG at the fastest compression level; if encoding exceeds about 30% of render time, switch to raw RGBA drawn into a `<canvas>`.
- Above a zoom threshold, render 512 px tiles instead of whole pages.
- Keep an LRU cache of rendered images keyed by document, page, scale bucket and revision, with a configurable memory cap.
- Render only pages within one screen of the viewport; show a sized placeholder for the rest.

**Text geometry.** For text selection and highlights, Rust extracts MuPDF's structured text per page (characters with their quads) and sends it to the frontend once per page and revision. The frontend does hit-testing and selection locally against that cached geometry, then sends the selected character range back with the highlight operation, so Rust computes the final quads.

**IPC surface** (initial; extend as needed, all typed): `open_document`, `close_document`, `get_document_info` (page count, page sizes and rotation, outline, page labels, annotation list), `get_page_text`, `search_text`, `apply_operation`, `undo`, `redo`, `save`, `save_as`, `plan_merge`, `execute_merge`, `scan_annotations_for_repair`, `repair_annotations`. Every mutating command returns the new document revision plus the changed data, so the frontend never re-fetches everything.

## 4. Repository layout

Use a Cargo workspace plus a SvelteKit app at the root. Keep this structure unless an ADR justifies a change.

```
folio/
├─ AGENTS.md                 # this document
├─ LICENSE                   # AGPL-3.0-or-later
├─ THIRD_PARTY_NOTICES.md
├─ Cargo.toml                # workspace
├─ crates/
│  ├─ pdf-core/              # all PDF logic, no Tauri dependency
│  │  ├─ src/
│  │  │  ├─ session.rs       # document actor, revisions
│  │  │  ├─ render.rs        # display lists, tiles, cache
│  │  │  ├─ text.rs          # structured text, search
│  │  │  ├─ geometry.rs      # view <-> PDF user space transforms
│  │  │  ├─ annot/           # write profile, appearance streams, repair
│  │  │  ├─ outline.rs       # bookmarks
│  │  │  ├─ labels.rs        # page labels
│  │  │  ├─ merge.rs         # stitching
│  │  │  ├─ save.rs          # incremental / full / atomic replace
│  │  │  ├─ ops.rs           # operation enum, undo/redo
│  │  │  └─ ffi/             # thin safe wrappers over mupdf-sys
│  │  └─ tests/
│  └─ pdf-cli/               # headless CLI over pdf-core (tests, debugging)
├─ src-tauri/                # Tauri app crate
├─ src/                      # SvelteKit frontend
│  ├─ lib/components/
│  ├─ lib/features/          # viewer/, bookmarks/, labels/, merge/, annotations/
│  ├─ lib/ipc/               # generated types + typed invoke wrappers
│  ├─ lib/stores/
│  └─ routes/
├─ tests/
│  ├─ corpus/                # real-world PDFs (Git LFS), read-only
│  ├─ interop/               # cross-renderer harness
│  └─ e2e/
└─ docs/
   ├─ decisions/             # ADRs: NNNN-title.md
   ├─ interop-profile.md     # the annotation write profile, kept in sync with section 5
   ├─ versions.md
   └─ progress.md            # phase reports
```

The `pdf-cli` tool exposes every `pdf-core` operation from the command line (for example `pdf-cli labels set in.pdf out.pdf --rule 0:roman-lower --rule 12:decimal`). Tests and the interop harness use it, and it makes engine bugs reproducible without the UI.

## 5. Annotation interoperability contract

Every annotation this app writes must display, print and be editable in Acrobat Reader, PDFium-based viewers (Edge, Chrome), pdf.js (Firefox) and Foxit. Write conservatively, read leniently, and never weaken these rules to make a test pass: report the conflict instead. Mirror this section in `docs/interop-profile.md`.

### 5.1 Write profile (mandatory for every annotation written or edited)

1. **Standard subtypes only:** Highlight, Underline, StrikeOut, Squiggly, Text (sticky note), Ink, FreeText. No custom subtypes and no private keys.
2. **Appearance stream always.** Every annotation gets a normal appearance (`/AP /N`), regenerated after every edit, using MuPDF's appearance synthesis. If MuPDF's appearance for a type fails the interop tests (section 9), write a custom appearance stream for that type in `annot/` and document why in an ADR.
3. **QuadPoints in Acrobat order.** For text-markup types, each quad is written as upper-left, upper-right, lower-left, lower-right, in PDF user space. This is the de facto order Acrobat uses, not the counter-clockwise order the spec text describes. One function writes quads, with unit tests.
4. **Rect contains everything.** `/Rect` is the union of all quads, ink paths, or the text box, plus the stroke width and a 1 pt margin.
5. **Highlights blend.** Highlight appearance streams use an ExtGState with Multiply blend mode so text stays readable. Store opacity in `/CA` on the annotation as well as in the appearance.
6. **Complete metadata:** `/NM` (a UUID), `/T` (author, from settings, defaulting to the Windows user name), `/CreationDate` and `/M` (PDF date strings with time zone), `/F 4` (Print flag), `/C` (color), `/P` (page reference). Sticky notes and markup with a note get a linked `/Popup` annotation with the `/Parent` back-reference.
7. **Plain FreeText.** Use the base-14 Helvetica font through `/DA`, one font size and one color per box, plain-text `/Contents`, and an appearance stream. Do not write `/RC` rich text in v1.
8. **Correct coordinates.** All conversions between screen and PDF user space go through `geometry.rs`, which handles `/Rotate` (0, 90, 180, 270), a CropBox whose origin is not (0, 0), and `/UserUnit`. Test each case with fixtures.
9. **Lean ink.** Simplify freehand strokes (Ramer–Douglas–Peucker, about 0.5 pt tolerance) before writing.
10. **Leave others' work alone.** Never rewrite, reorder or drop annotations the user did not touch. When the user edits an annotation made by another app, change only the edited keys plus its appearance, and keep unknown keys.

### 5.2 Reading other apps' annotations

- Display every standard annotation type MuPDF supports, including ones this app cannot create (shapes, stamps, links, file attachments).
- If an annotation lacks an appearance stream or has malformed quads, draw it for display from its properties, but write nothing. Mark it in the annotation list with a "needs repair" badge.
- Show replies (`/IRT`) under their parent in the list, read-only in v1.

### 5.3 Repair annotations command

A command that scans the document, shows a summary of problems found (counts per problem type), and on confirmation fixes them in one undoable operation:

- generate missing appearance streams;
- reorder QuadPoints into Acrobat order;
- recompute `/Rect` where it does not contain the content;
- add missing `/NM`, `/F 4`, `/M`, and `/P`.

Repair must never change an annotation's content, color, author or position. Log every change to the app log with the annotation's object number.

### 5.4 Saving rules that protect interoperability

- Default save is incremental: changes are appended and the original bytes stay intact.
- If MuPDF reports the file was repaired on open (damaged cross-reference table), it cannot be saved incrementally; do a full save and tell the user once, in plain words, why.
- "Save As (optimized)" does a full rewrite with garbage collection and stream compression.
- If the document is digitally signed, warn before the first edit that edits will be shown as changes after signing, and always save incrementally.
- Encrypted documents: prompt for the password, keep the encryption on save, and respect permission flags (no annotating if the document disallows it).

## 6. Feature specifications

Each feature below defines behavior; section 10 defines when it counts as done.

### 6.1 Viewer

- **Opening:** File > Open dialog, drag-and-drop onto the window, `.pdf` file association, and a path passed on the command line. Opening a file that is already open switches to its tab.
- **Tabs:** one tab per document, reorderable, with a dirty marker and a close prompt for unsaved changes.
- **Scrolling:** continuous vertical scroll, virtualized; only pages near the viewport are rendered.
- **Zoom:** fit width, fit page, preset percentages, Ctrl+wheel and pinch, centered on the cursor.
- **Navigation:** page box accepts a physical page number or a page label (typing `iv` jumps to the page labeled iv); thumbnails sidebar; back/forward history for jumps (Alt+Left/Right).
- **Text:** selection and copy across lines and pages; search with match highlighting and next/previous.
- **View rotation:** rotating the view does not modify the document. A separate "Rotate pages" command does modify it (sets `/Rotate`) and is undoable.
- **Recent files** list and remembered per-file view position (page and zoom), stored in app data, not in the PDF.

### 6.2 Bookmarks (outline)

- Sidebar panel with the outline as a tree. Expanded/collapsed state is read from and written to the outline.
- **Add bookmark** (Ctrl+B) at the current page and scroll position; if text is selected, use it as the title.
- **Edit:** rename inline (F2 or double-click), drag to reorder and nest, delete (with children, after confirmation), "Set destination to current view."
- New and retargeted bookmarks get explicit destinations: page reference, `/XYZ`, left and top of the current view, null zoom (keeps the reader's zoom).
- Existing bookmarks the user did not edit keep their original destination or action exactly, including named destinations and URI actions.
- Titles with non-ASCII characters are written as UTF-16BE with a byte-order mark.

### 6.3 Page labels

- Edited as a list of rules. Each rule has a start page, a style (none, 1 2 3, i ii iii, I II III, a b c, A B C), an optional prefix, and a start number (1 or higher).
- A rule at the first page always exists (default: decimal from 1), because the format requires it.
- **Live preview:** thumbnail captions and the page box update as rules change, before saving.
- **Presets:** "Roman front matter, then arabic from this page" and "Remove all labels."
- Letter styles follow the PDF convention: a…z, then aa…zz, then aaa…zzz.
- Opening a file shows its existing labels as rules exactly as stored; saving writes the `/PageLabels` number tree in the document catalog.

### 6.4 Stitching (combine files)

- A Combine view: add files (dialog or drag-and-drop), see all their pages as thumbnails in one grid, and reorder, remove or rotate pages before combining.
- **Bookmark option** (default first): nest each source's outline under a new top-level bookmark named after the source's title or file name; or keep outlines flat; or drop them.
- **Page label option:** keep each source's labels (default); one continuous decimal sequence; or no labels.
- Annotations travel with their pages and still pass the interop profile afterwards.
- Internal links and named destinations are remapped to the new pages; colliding destination names and form field names are renamed with a per-source prefix, and the user is told how many were renamed.
- The result is always written to a new file via Save As. Source files are never modified.
- Also expose "Insert pages from file" into an open document, using the same engine.

### 6.5 Annotations (user interface)

- **Toolbar tools:** Select, Highlight, Underline, Strikeout, Squiggly, Note, Pen, Text box.
- **Text markup:** with a markup tool active, selecting text creates the annotation on mouse-up. Alt-drag creates an area highlight (one rectangular quad) for scanned pages without text.
- **Style:** six preset colors plus a custom picker, opacity, stroke width for the pen; the last-used style per tool is remembered.
- **Inspector panel** for the selected annotation: color, opacity, note text, author, dates.
- **Annotation list** in the sidebar: grouped by page, filter by type and author, click to jump, edit note text, delete, "needs repair" badges.
- Annotations can be moved, resized (ink, text box, notes) and deleted; every change is undoable.
- Author name is set in Settings, defaulting to the Windows user name.

## 7. Document model, undo/redo and saving

The Rust session is the single source of truth: the frontend holds only a view of it, and the undo history lives in Rust.

**Operations.** Every change is a variant of one `Operation` enum in `ops.rs` (for example `AddAnnotation`, `UpdateAnnotation`, `DeleteAnnotation`, `SetOutline`, `SetPageLabels`, `RotatePages`, `InsertPages`, `RepairAnnotations`). Applying an operation increments the document revision and returns what changed.

**Undo/redo.** Prefer MuPDF's built-in journalling (the begin/end operation and undo/redo functions in its PDF API), wrapping each `Operation` as one journal step with a human-readable name ("Add highlight", "Rename bookmark"). If the Rust crate does not expose journalling, wrap it in `ffi/`. Only if journalling proves unusable, implement inverse operations instead, and record that decision in an ADR. The Edit menu shows the name of the step being undone or redone.

**Dirty state.** A document is dirty when its revision differs from the last saved revision. Show a dot in the tab, and prompt on close and on app exit.

**Atomic save.**

1. Write the new file to a temporary file in the same folder as the target.
2. Flush it to disk.
3. Replace the original in one rename on the same volume.
4. On failure, delete the temp file and leave the original untouched.

If the target is locked by another program (Acrobat locks files it has open), show a clear message naming the likely cause and offer Save As.

**Crash recovery.** Every 2 minutes while dirty, write a recovery copy to the app's local data folder. On the next launch, offer to restore any recovery copies that exist, then delete them.

**External changes.** Watch open files. If a file changes on disk and the document has no unsaved edits, offer to reload; if it has unsaved edits, warn and offer Save As.

## 8. UI and UX guidelines

The app should feel like a native Windows 11 app: calm, fast, and keyboard-friendly, with the document always the visual focus.

**Layout.**

- **Title bar:** custom (Tauri decorations off, explicit drag region), holding the document tabs and the standard window buttons.
- **Left sidebar**, collapsible, with four panels: Thumbnails, Bookmarks, Annotations, Page labels.
- **Center:** the page canvas, with a floating annotation toolbar.
- **Right inspector**, shown only when something is selected: properties of the selected annotation or bookmark.
- **Status bar:** page label and physical page number (e.g. "iv (4 of 312)"), zoom level, save state.

**Visual style.**

- Mica window background through Tauri's window effects, with a solid fallback where Mica is unavailable.
- Follow the system light/dark setting and accent color by default; Settings can force light or dark (Phase 5).
- Font stack: Segoe UI Variable, Segoe UI, then system UI fonts for other platforms.
- Spacing on an 8 px grid (4 px for tight spots); corner radius 6–8 px; thin borders instead of heavy shadows.
- All colors, sizes and radii as CSS variables (design tokens) consumed by Tailwind; no hard-coded colors in components.

**Behavior.**

- Never block the UI thread. Long operations (combining, repair, saving a large file) show progress and can be canceled.
- Placeholder page shapes at the correct size while pages render, so the scroll position never jumps.
- Errors in plain language with a suggested next step; never show raw error text or crash on bad input. Log details to a rotating log file.
- Respect reduced-motion settings; keep animations under 150 ms.

**Accessibility.** Every control is reachable by keyboard with a visible focus ring, has an accessible name, and meets WCAG AA contrast.

**Default shortcuts** (all rebindable later, not in v1):

| Action | Shortcut |
| --- | --- |
| Open / Save / Save As | Ctrl+O / Ctrl+S / Ctrl+Shift+S |
| Undo / Redo | Ctrl+Z / Ctrl+Y |
| Find | Ctrl+F |
| Add bookmark | Ctrl+B |
| Zoom in / out / fit width | Ctrl+= / Ctrl+- / Ctrl+0 |
| Go to page | Ctrl+G |
| Next / previous tab | Ctrl+Tab / Ctrl+Shift+Tab |
| Select / Highlight / Underline / Note / Pen / Text box tool | Esc / H / U / N / P / T |
| Rename bookmark | F2 |
| Delete selection | Del |

## 9. Testing strategy and interop harness

Interoperability is verified by machines on every commit and by a person before every release; neither replaces the other.

**Test corpus** (`tests/corpus/`, Git LFS, never modified by tests). Collect at least one file per category:

- rotated pages (90, 180, 270) and a CropBox with a non-zero origin;
- a damaged cross-reference table, an encrypted file, a signed file;
- a large file (1,000+ pages) and a scanned file with no text layer;
- CJK and right-to-left text;
- files with existing outlines, existing page labels, and form fields;
- annotations made by other apps, especially the ones that display wrongly in Acrobat (the user will supply these).

Write test outputs to `target/test-output/`, never next to corpus files.

Real-world files the user owns but cannot publish (for example books from their Calibre library) form a **local corpus**: `tests/local-corpus/survey.py` profiles a library read-only, `select.py` copies one or two files per category into `tests/local-corpus/files/` and writes `manifest.json` (both git-ignored), and `crates/pdf-core/tests/local_corpus.rs` runs over them, skipping when they are absent (as in CI). The source library is only ever read, never written.

**Unit tests (`pdf-core`):** coordinate transforms for every rotation and CropBox case; quad ordering; label formatting (roman, letters past z, prefixes); outline serialization including Unicode titles; atomic save failure paths.

**Round-trip tests:** for each annotation type, create → save → reopen with MuPDF → assert every key the write profile requires, with the expected values. Same for outlines, labels and merges.

**Structural check:** run `qpdf --check` on every file the tests produce; any warning fails the test.

**Cross-renderer harness (`tests/interop/`):**

1. For each test output, render each annotated page with three independent engines: MuPDF (`mutool draw`), PDFium (through a small script using the `pypdfium2` package), and pdf.js (a Node script using `pdfjs-dist`).
2. Inside each annotation's `/Rect`, compare the render against the same page rendered without annotations. If any engine shows no visible change, the test fails: that annotation is invisible in that engine.
3. Compare engines against each other with a tolerance, and save diff images for any failure.

SumatraPDF and similar apps are built on MuPDF and do not count as an independent check.

**Manual release checklist** (performed by the user; the agent prepares the files and a checklist document): open the interop sample files in Acrobat Reader, Edge, Firefox and Foxit; for every annotation type confirm it is visible, has the right color and opacity, appears in the comments list, prints, and can be edited and deleted.

**Frontend and end-to-end:** Vitest for stores and component logic; `tauri-driver` with WebdriverIO for the critical flows (open, highlight, add bookmark, set labels, combine, save, reopen).

**CI** (GitHub Actions, `windows-latest`): `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test`, `svelte-check`, Vitest, the interop harness, and an installer build uploaded as an artifact. Add macOS and Linux build-only jobs once Phase 6 starts.

## 10. Build phases

Build in seven phases, in order. A phase is done only when every box below is checked and CI is green; then write a report in `docs/progress.md` and stop for the user's review before starting the next phase.

### Phase 0: Feasibility spike

Goal: prove the risky parts before building UI.

- [ ] Tauri 2 + SvelteKit (static, SSR off) + Tailwind skeleton builds and runs on Windows.
- [ ] MuPDF linked through the `mupdf` crate; MuPDF version pinned and recorded.
- [ ] A page renders in the window through the `folio://` protocol; render and encode times measured on a 500-page file.
- [ ] `pdf-cli` can write page labels, write an outline, merge two files, and add a highlight with an appearance stream.
- [ ] Those four outputs pass `qpdf --check` and display correctly in the interop harness (all three engines).
- [ ] Journalling (undo/redo) works on at least one operation, through the crate or `ffi/`.
- [ ] ADR listing which MuPDF APIs the crate covers, which needed `ffi/` wrappers, and the thread-safety findings.
- [ ] Performance numbers from section 2 measured and reported, even if not yet met.

### Phase 1: Viewer

- [ ] Everything in 6.1 works.
- [ ] Scrolling a 1,000-page file shows no blank page for more than 200 ms on a mid-range laptop.
- [ ] Atomic save, dirty state, close prompts and external-change detection (section 7) work.
- [ ] Layout, theming and shortcuts from section 8 are in place for the viewer parts.

### Phase 2: Bookmarks

- [ ] Everything in 6.2 works, with undo/redo for each action.
- [ ] A file with a 3-level outline edited in the app opens with the correct tree, titles and targets in Acrobat, Edge and Firefox.
- [ ] Untouched bookmarks keep their original destinations byte-for-byte (verified by a test).
- [ ] End-to-end tests (`tauri-driver` with WebdriverIO, section 9) run in CI for open, save and reopen, and for adding, renaming and saving a bookmark.
- [ ] WebView2 memory (ADR 0002 growth rule, Phase 1 report) investigated: where the growth comes from (GPU process, renderer), what reduces it (fewer mounted pages, lower-resolution images while scrolling fast, `<img>` instead of `<canvas>`, WebView2's memory target level), and either a fix that meets the rule or a proposal to revise it.
- [ ] Fast lookup of non-embedded fonts (Phase 1 local-corpus finding): an index of installed fonts built once on the background warm-up thread (DirectWrite on Windows, behind a platform trait; family, style and PostScript name without loading font files) replaces `font-kit` and the `mupdf` crate's `system-fonts` feature, including the CJK fallback families. ADR first. Local-corpus files with non-embedded fonts show their first page in under 1 s and render the same fonts as before.

### Phase 3: Page labels

- [ ] Everything in 6.3 works, with undo/redo.
- [ ] Labels written by the app show correctly in Acrobat's page box and thumbnails.
- [ ] Existing labels from corpus files round-trip unchanged when the user makes no label edits.

### Phase 4: Stitching

- [ ] Everything in 6.4 works.
- [ ] Combining three corpus files (one with outline, one with labels, one with annotations) produces a file that passes `qpdf --check`, keeps the annotations visible in all three engines, and has working internal links.
- [ ] Combining two 500-page files completes with a progress bar and can be canceled without leaving partial files.

### Phase 5: Annotations and repair

- [ ] Every annotation type in 6.5 can be created, edited, moved and deleted, with undo/redo.
- [ ] Every rule in 5.1 is enforced and covered by a round-trip test.
- [ ] The interop harness passes for every annotation type on rotated, cropped and normal pages.
- [ ] The repair command (5.3) fixes the user-supplied problem files so they pass the harness, without changing content, color, author or position.
- [ ] The manual checklist in section 9 is prepared for the user.
- [ ] A Settings dialog (author name, 6.5) with an appearance choice: System (default), Light or Dark; the choice applies immediately and is remembered in app data.

### Phase 6: Polish and release

- [ ] Crash recovery, recent files and per-file view memory work.
- [ ] NSIS and MSI installers build in CI; `.pdf` file association can be chosen at install.
- [ ] Performance targets from section 2 met, or gaps documented with a plan.
- [ ] Large JPEG 2000 pages (Phase 2 finding): a page holding a 33-megapixel JPEG 2000 scan takes about 1.8 s to appear, almost all of it decoding the full image. Look into decoding such images at the resolution shown, and report the result.
- [ ] Accessibility pass done (keyboard-only walkthrough, contrast check).
- [ ] macOS and Linux build-only CI jobs added; platform gaps listed in `docs/progress.md`.

## 11. Working rules for the agent

Follow these rules in every session; when a rule and a convenience conflict, the rule wins.

**Process.**

- Start each session by reading this document and `docs/progress.md`.
- Work on one phase at a time; never start a phase before the previous one is reviewed.
- Make small commits using Conventional Commits (`feat:`, `fix:`, `test:`…). Every commit builds and passes tests.
- When you deviate from this document, write an ADR in `docs/decisions/` first, explaining what, why, and the alternatives considered.

**MuPDF.**

- Never guess MuPDF function names or behavior. Check the pinned version's headers and documentation before using any API.
- Where the `mupdf` crate lacks an API, add the smallest safe wrapper in `pdf-core/src/ffi/`, with a `// SAFETY:` comment on every `unsafe` block and a test for the wrapper.
- Keep `pdf-core` free of any Tauri dependency.

**Code standards.**

- Rust: no `unwrap()` or `expect()` outside tests unless the invariant is documented beside it; errors via `thiserror`; no panics across the IPC boundary.
- TypeScript: strict mode, no `any`, IPC types generated from Rust only.
- Svelte: runes for state; feature code in `src/lib/features/<feature>/`.

**Dependencies.** Add a dependency only when it saves significant work. Note its license (must be AGPL-compatible) and its size impact in the commit message, and update `THIRD_PARTY_NOTICES.md`.

**Tests and data.**

- Never modify files in `tests/corpus/`.
- Never weaken the interop profile, a test, or a threshold to get a green build. Report the problem and the options instead.

**Stop and ask the user when:**

- a decision affects licensing or adds a non-trivial dependency;
- a UI or behavior question is not answered by this document;
- a request or idea falls outside the v1 scope in section 1;
- an acceptance criterion seems impossible as written.
