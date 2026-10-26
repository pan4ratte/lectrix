# Lectrix — Instructions for Coding Agents

Oct 4, 2026 · @Mark

## 1. Mission and scope

Lectrix is a lightweight, open-source PDF editor for Windows, designed to go cross-platform later, whose annotations, bookmarks and page labels open correctly in Acrobat and every other mainstream reader. Interoperability is the product's core promise: a feature that works only inside this app is a failed feature.

This document and the feature specifications in `docs/spec/` are the single source of truth for the product and for how to work on it (ADR 0022). Read this document in full at the start of every session, and the spec of a feature before changing it. v1 is complete (2026-10-04); `docs/status.md` lists its measurements and known gaps. The name is **Lectrix**; the frontend keeps it in one config constant (`APP_NAME` in `src/lib/config.ts`).

**v1 contains:**

- **Viewer:** fast continuous scrolling, zoom, thumbnails, text selection, text search, tabs for multiple documents, which can be spread over several windows.
- **Bookmarks:** create, rename, reorder, nest, delete, and retarget the document outline.
- **Page labels:** assign label ranges (roman front matter, arabic body, prefixed appendices).
- **Stitching:** combine several PDFs into one, at page level, with bookmarks and labels merged sensibly.
- **Annotations:** highlight, underline, strikeout, squiggly, sticky note, freehand ink, text box, plus an annotation list.
- **Repair annotations:** normalize broken annotations written by other apps so they display everywhere.

**Not in v1** (do not build, even partially, without the user's go-ahead): form filling, digital signing, OCR, editing page text or images, redaction, converting other formats to PDF, cloud sync, mobile builds.

## 2. Tech stack and constraints

The stack is fixed: Tauri 2, a Rust backend running MuPDF, and a Svelte 5 frontend styled with Tailwind. Versions are pinned exactly and recorded in `docs/versions.md`; upgrade deliberately and update that file.

| Layer | Choice | Notes |
| --- | --- | --- |
| App shell | Tauri 2 | WebView2 on Windows. Bundled with the Tauri bundler (an NSIS installer, ADR 0020). |
| Backend language | Rust, stable toolchain | All PDF logic lives here. |
| PDF engine | MuPDF (C library) | Via the `mupdf` Rust crate, vendored and patched in `third_party/mupdf-rs` (ADR 0001); `mupdf-sys` comes from Lectrix's fork (ADR 0008), which also has MuPDF decode JPEG with libjpeg-turbo (ADR 0024; NASM is a build requirement). The C API is called directly through `mupdf-sys` where the crate lacks coverage. |
| Frontend | Svelte 5 (runes) on SvelteKit | `adapter-static`, SSR disabled, as Tauri's own SvelteKit guide recommends. TypeScript in strict mode. |
| Styling | Tailwind CSS | Through its Vite plugin. Design tokens as CSS variables. |
| UI primitives | Bits UI (headless) and Lucide icons | Headless only, so the visual design stays ours. |
| Rust/TS type sharing | `ts-rs` | TypeScript types for every IPC command and payload are generated (`cargo test -p lectrix`); never hand-write them. |
| Testing | `cargo test`, Vitest, `tauri-driver` with WebdriverIO | See section 9. |

**Hard constraints:**

- **License:** the whole project is AGPL-3.0-or-later, because MuPDF is AGPL. Every new dependency must be AGPL-compatible (MIT, Apache-2.0, BSD, MPL-2.0 are fine). Maintain `THIRD_PARTY_NOTICES.md`; `tests/licenses/notices.py` checks every shipped package and regenerates `THIRD_PARTY_LICENSES.md`. The About dialog shows where the source is published (`SOURCE_URL` in `src/lib/config.ts`).
- **Platforms:** Windows 10 (1809+) and Windows 11, x64 first. Anything Windows-specific goes in a `platform` module behind a trait, so macOS and Linux can be added without touching feature code.
- **Network:** none at runtime except the update check and download from Lectrix's GitHub releases (ADR 0011): once a day, which Settings can turn off, and when the user picks Help > Check for updates. No telemetry, and the webview never goes online. A web link the user clicks in a document, like the links in About, is handed to the system's browser (section 6.1); Lectrix itself fetches nothing.
- **Security:** minimal Tauri capabilities. File access only to paths the user picked through a dialog, drag-and-drop, or file association. Strict CSP, no remote content in the webview.
- **Performance targets** (a regression is a bug; the latest measurements are in `docs/status/performance.md`): window visible in under 1 s; first page of a 500-page PDF visible in under 1 s; no blank page visible for more than 200 ms while scrolling; idle memory under 200 MB with one large document open (for `lectrix.exe`; WebView2 has its own rule, ADR 0002).

## 3. Architecture

All PDF reading and writing happens in Rust through MuPDF; the Svelte frontend never parses or modifies PDF bytes. The frontend displays rendered pages and draws interactive overlays, and every change it makes is a typed operation sent to Rust. The rendering pipeline, text geometry, documents and views, windows and the IPC surface are described in [docs/architecture.md](docs/architecture.md).

- **Three layers:** `pdf-core` (a plain Rust library with no Tauri dependency: all PDF logic, everything testable headless), `src-tauri` (a thin layer: IPC commands, the page-image protocol, dialogs, menus, windows, app state, logging) and the SvelteKit frontend (views, editors, overlays and UI state only).
- **Threading:** each open document has its own actor thread that exclusively owns its MuPDF document; commands reach it through a channel. Parallel rendering uses display lists on a worker pool with cloned contexts (ADR 0001).
- **Pages** come from the `lectrix://page/…` protocol, whose URL carries the document revision, so cache invalidation is automatic. **Text geometry** comes to the frontend once per page and revision; the frontend hit-tests and selects locally, and Rust computes the final quads.
- **IPC:** commands live in `src-tauri/src/commands.rs`, all typed (`ts-rs` generates their TypeScript; never hand-write it). Every mutating command returns the new revision plus the changed data. A new command is declared in `src-tauri/build.rs` and granted in `src-tauri/capabilities/default.json` (a test checks both), or calling it fails at run time.
- **Windows:** every window is the whole app. Rust sends an event to the window it concerns (`emit_to`), and the frontend listens through its own window (`listen` in `src/lib/ipc/index.ts`).

## 4. Repository layout

A Cargo workspace plus a SvelteKit app at the root; keep this structure unless an ADR justifies a change. The full tree is in [docs/architecture.md](docs/architecture.md).

- `crates/pdf-core/`: all PDF logic, no Tauri dependency (`ops.rs` the operations, `session.rs` the document actor, `annot/` the write profile, `ffi/` wrappers over `mupdf-sys`, `testgen.rs` generated samples).
- `crates/pdf-cli/`: a headless CLI exposing every `pdf-core` operation (for example `pdf-cli labels set in.pdf out.pdf --rule 0:roman-lower --rule 12:decimal`); tests and the interop harness use it.
- `src-tauri/`: the Tauri app crate. `src/`: the frontend, with feature code in `src/lib/features/<feature>/` and IPC types and wrappers in `src/lib/ipc/`.
- `third_party/`: the vendored mupdf crate and MuPDF headers. `tests/`: corpus, interop harness, e2e, perf, soak. `docs/`: ADRs, specs, status, versions, the interop profile, the release checklist.

## 5. Annotation interoperability contract

Every annotation this app writes must display, print and be editable in Acrobat Reader, PDFium-based viewers (Edge, Chrome), pdf.js (Firefox) and Foxit. Write conservatively, read leniently, and never weaken these rules to make a test pass: report the conflict instead. The rules in full, with where each is enforced and tested, are in [docs/interop-profile.md](docs/interop-profile.md); keep it and this section in step.

### 5.1 Write profile (mandatory for every annotation written or edited)

1. **Standard subtypes only:** Highlight, Underline, StrikeOut, Squiggly, Text (sticky note), Ink, FreeText. No custom subtypes and no private keys.
2. **Appearance stream always** (`/AP /N`), regenerated after every edit, by MuPDF's synthesis with the corrections in ADR 0006; a type whose MuPDF appearance fails the interop tests gets a custom one in `annot/`, with an ADR.
3. **QuadPoints in Acrobat order:** upper-left, upper-right, lower-left, lower-right, as the glyphs stand (in right-to-left text too, ADR 0017), in PDF user space, written by one function.
4. **Rect contains everything:** the union of all quads, ink paths or the text box, plus the stroke width and a 1 pt margin.
5. **Highlights blend** (Multiply, in an ExtGState); opacity in `/CA` on the annotation as well as in the appearance.
6. **Complete metadata:** `/NM` (a UUID), `/T`, `/CreationDate` and `/M` (with time zone), `/F 4`, `/C` (for FreeText the background, so `/C []` and the text color in `/DA`), `/P`; notes and markup with a note get a `/Popup` with `/Parent`.
7. **Plain FreeText:** base-14 Helvetica through `/DA`, one size and color, plain-text `/Contents`, no `/RC`.
8. **Correct coordinates:** every screen ↔ PDF conversion goes through `geometry.rs` (`/Rotate`, a CropBox not at (0, 0), `/UserUnit`), with a fixture for each case.
9. **Lean ink:** strokes simplified (Ramer–Douglas–Peucker, about 0.5 pt) before writing.
10. **Leave others' work alone:** never rewrite, reorder or drop annotations the user did not touch; an edit of another app's annotation changes only the edited keys plus its appearance, and keeps unknown keys.

### 5.2 Reading other apps' annotations

Display every standard type MuPDF supports. One without an appearance, or with malformed quads, is drawn for display only, nothing written, and badged "needs repair" in the list; mirrored QuadPoints are no problem, crossed ones are (ADR 0017). Replies (`/IRT`) show in their parent's thread (ADR 0012).

### 5.3 Repair annotations command

Scans, summarizes the problems, and on confirmation fixes them in one undoable operation: missing appearances, crossed QuadPoints, a `/Rect` that does not contain the content, missing `/NM`, `/F 4`, `/M`, `/P`. It never changes an annotation's content, color, author or position, and logs every change with the object number.

### 5.4 Saving rules that protect interoperability

Incremental by default. A file repaired on open gets a full save, and the user is told why, once. Signed files: warn before the first edit, always save incrementally. Encrypted files keep their encryption and their permission flags are respected, except through Unlock and Protect. PDF/A files open for reading only until Enable editing, and a save whose changes may break the claim removes it. All of it in full: [docs/interop-profile.md](docs/interop-profile.md), "Saving".

## 6. Feature specifications

What each feature does is specified in `docs/spec/`, one file per feature, under the section numbers below, so a reference such as "section 6.5" leads to its file. Read a feature's spec before changing it, and change the spec in the same commit as its behavior. The "settled details" in each were decided during the v1 build; change them only with the user's agreement.

- **6.1 Viewer** ([viewer.md](docs/spec/viewer.md)): opening, tabs and windows, split view, scrolling, zoom, navigation, selection, search, links, full screen, visual adjustments, the start screen. What is display only (view rotation, pages turned on screen, visual adjustments) is never written to the file. A web link is opened only by Rust (`open_link`), only if a link on that page carries it, and only an http or https address.
- **6.2 Bookmarks** ([bookmarks.md](docs/spec/bookmarks.md)) · **6.3 Page labels** ([page-labels.md](docs/spec/page-labels.md)) · **6.4 Stitching** ([combine.md](docs/spec/combine.md); the sources are never modified).
- **6.5 Annotations** ([annotations.md](docs/spec/annotations.md)): the tools, colors, the bar, the inspector, the annotation list. Every change is one undo step.
- **6.6 Settings, About and installers** ([settings.md](docs/spec/settings.md)): a new setting is a field in Rust's `Settings` plus a row in `src/lib/features/settings/schema.ts`; a new command with a shortcut is an entry in `SHORTCUT_GROUPS` (`src/lib/shortcuts.ts`); only what differs from the defaults is stored.
- **6.7 Organize pages** ([organize.md](docs/spec/organize.md)): each change is one undo step (ADR 0018), and what is kept by page follows its page (`DocumentChange.pageMap`).
- **6.8 Unlock** ([unlock.md](docs/spec/unlock.md)) · **6.9 Protect** ([protect.md](docs/spec/protect.md)) · **6.10 Document properties** ([properties.md](docs/spec/properties.md)) · **6.11 Compress** ([compress.md](docs/spec/compress.md)) · **6.12 Print** ([print.md](docs/spec/print.md)).

## 7. Document model, undo/redo and saving

The Rust session is the single source of truth: the frontend holds only a view of it, and the undo history lives in Rust. Dirty state, documents without a file, the atomic save step by step, crash recovery and external changes are described in [docs/document-model.md](docs/document-model.md).

- **Operations:** every change is a variant of one `Operation` enum in `ops.rs`. Applying one increments the document revision and returns what changed.
- **Undo/redo:** MuPDF's journalling (`ffi/journal.rs`); each `Operation` is one journal step with a human-readable name ("Add highlight"). Saving reopens the file, so undo history starts again after each save (ADR 0003).
- **Atomic save:** write a temporary file in the target's folder, flush it, replace the original in one rename. On failure the original and the document in memory stay as they were, so saving again is safe.

## 8. UI and UX guidelines

The app should feel like a native Windows 11 app: calm, fast and keyboard-friendly, with the document always the visual focus. The layout, controls, motion, visual style and default shortcuts are in [docs/spec/ui.md](docs/spec/ui.md) (sections 8.1 to 8.5).

- **Controls are the app's own:** dropdowns (`Dropdown.svelte`, never `<select>`), sliders (`Slider.svelte`), tooltips (`data-tip`, never `title`; a test checks).
- **Design tokens:** colors, sizes and radii are CSS variables consumed by Tailwind; no hard-coded colors in components.
- **Never block the UI thread:** long operations show progress and can be canceled. Pages render into placeholders of the right size, so the scroll position never jumps.
- **Errors** in plain language with a suggested next step, never raw error text; details go to the log.
- **Motion:** respect reduced-motion settings; animations stay under 150 ms (the start screen's cards excepted, ADR 0030).
- **Accessibility:** every control reachable by keyboard, with a visible focus ring and an accessible name, and WCAG AA contrast (`src/lib/contrast.test.ts`); Shift+F10 and the Menu key open the context menu; app-wide shortcuts work while typing in panel fields.

## 9. Testing

Interoperability is verified by machines on every commit and by a person before every release; neither replaces the other. The corpus, the interop harness and its suites, the debug menu, CI and the soak run are described in [docs/testing.md](docs/testing.md).

- **Run:** `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test` (`cargo test -p lectrix` regenerates the IPC types), `npm run check`, `npx vitest run`. End-to-end: build the release app (`npx tauri build --no-bundle`), then `npm run test:ci --prefix tests/e2e -- specs/<name>.test.mjs`, which shows the app as CI's runners do (1024 x 768, reduced motion); run them that way before pushing.
- **Write:** round-trip tests for each annotation type, outlines, labels and merges; `qpdf --check` on every file a test produces (any warning fails); test outputs go to `target/test-output/`.
- **Never** modify files in `tests/corpus/`, and never weaken the interop profile, a test or a threshold to get a green build.
- The user performs the manual release checklist (`docs/manual-checklist.md`); keep it current. A new kind of notification gets an item in the Debug menu (`src/lib/features/debug/`).

## 10. Working rules for the agent

Follow these rules in every session; when a rule and a convenience conflict, the rule wins.

**Process.**

- Start each session by reading this document and `docs/status.md` (an index of where things stand, one line per gap); before changing a feature, read its gaps in full in the file under `docs/status/` that the index names. Before changing a feature, read its spec in `docs/spec/` (section 6), and `docs/spec/ui.md` before changing the layout or a shared control.
- Work on one change at a time. For a new feature or a larger change, propose a plan and wait for the user's go-ahead before building.
- When a change is done and CI is green, report to the user: what was built, the tests, any measurements that changed, behavior choices made without explicit guidance, and decisions needed. Then stop for review.
- Add every change users will notice to `CHANGELOG.md`, under `## Unreleased`, in the commit that makes it, worded as `docs/releasing.md` says.
- Keep the docs current: `docs/status.md` and `docs/status/` (add gaps you find, remove gaps you close: a line in the index, the detail in its file), `docs/versions.md`, `docs/interop-profile.md`, `docs/manual-checklist.md`, and, when the product's behavior changes, the spec in `docs/spec/` that describes it (this document only for what it covers: scope, constraints, architecture, the interop contract, saving, testing and these rules). Keep this document short: a feature's details go in its spec, not here.
- Make small commits using Conventional Commits (`feat:`, `fix:`, `test:`…). Every commit builds and passes tests.
- When you deviate from this document or a spec, write an ADR in `docs/decisions/` first, explaining what, why, and the alternatives considered.

**MuPDF.**

- Never guess MuPDF function names or behavior. Check the pinned version's headers (`third_party/mupdf-include/`) and documentation before using any API.
- Where the `mupdf` crate lacks an API, add the smallest safe wrapper in `pdf-core/src/ffi/`, with a `// SAFETY:` comment on every `unsafe` block and a test for the wrapper.
- Changes to the vendored crate are listed in `third_party/mupdf-rs/LECTRIX_PATCHES.md`; changes to MuPDF itself go through the `mupdf-rs` fork (ADR 0008), with a plain patch in `docs/upstream/`.
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
- an acceptance criterion or target seems impossible as written.
