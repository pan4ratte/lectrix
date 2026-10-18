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
| PDF engine | MuPDF (C library) | Via the `mupdf` Rust crate, vendored and patched in `third_party/mupdf-rs` (ADR 0001); `mupdf-sys` comes from Lectrix's fork (ADR 0008). The C API is called directly through `mupdf-sys` where the crate lacks coverage. |
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
- **Performance targets** (a regression is a bug; the latest measurements are in `docs/status.md`): window visible in under 1 s; first page of a 500-page PDF visible in under 1 s; no blank page visible for more than 200 ms while scrolling; idle memory under 200 MB with one large document open (for `lectrix.exe`; WebView2 has its own rule, ADR 0002).

## 3. Architecture

All PDF reading and writing happens in Rust through MuPDF; the Svelte frontend never parses or modifies PDF bytes. The frontend displays rendered pages and draws interactive overlays, and every change it makes is a typed operation sent to Rust.

&#91;embedded content: architecture · three layers over MuPDF\]

Down-arrows carry typed operations and calls; up-arrows carry page images, revisions and results.

**Three layers:**

1. **`pdf-core` crate** (plain Rust library, no Tauri dependency). Owns document sessions, rendering, text geometry, the annotation write profile, outlines, page labels, merging, repair, undo/redo, saving and crash-recovery copies. Everything in it must be testable headless.
2. **`src-tauri` app crate.** A thin layer: IPC commands that call `pdf-core`, the page-image protocol, file dialogs, menus, window effects, file association, recovery slots, app state, logging.
3. **SvelteKit frontend.** Views, editors, overlays and UI state only.

**Threading.** MuPDF contexts and documents must not be shared across threads without care. Each open document has its own actor thread that exclusively owns its MuPDF document; commands reach it through a channel and get replies back. For parallel rendering, build a display list per page on the actor thread, then render display lists on a small worker pool with cloned contexts, as MuPDF's multi-threading guide describes. ADR 0001 records the crate's thread-safety findings.

**Rendering pipeline.**

- The frontend requests pages through a custom URI protocol: `lectrix://page/{docId}/{pageIndex}?scale={s}&rev={r}`. Including the document revision in the URL makes cache invalidation automatic. `&hide={n},{n}…` names annotations to leave out of the image, by object number: the ones the annotation list filters out (section 6.5).
- Rust renders the page with MuPDF and returns a PNG at the fastest compression level (ADR 0004). Raw RGBA drawn into a `<canvas>` stays available for measurement (`LECTRIX_IMAGE_FORMAT`).
- Above a zoom threshold, render 512 px tiles instead of whole pages.
- The start screen's grid gets recent files' first pages from `lectrix://recent/{index}?w={px}&opened={ms}`: rendered from the file on disk without opening it as a document, named by the file's place in the recent list and the time it was opened (never by a path from the webview), and kept in memory while the file's size and modification time are unchanged.
- Keep an LRU cache of rendered images keyed by document, page, scale bucket and revision, with a configurable memory cap.
- Mount only pages near the viewport (a quarter screen above and below, ADR 0002); show a sized placeholder for the rest. Thumbnails wait while pages on screen render.
- Render ahead: once the view has rested for 120 ms, the frontend has Rust render the row of pages after the mounted ones and the row before them into the image cache (the page URL with `&warm=1`, which answers with no image), so a page scrolled or turned to is there already; a scanned page takes a quarter of a second to render. These pages are not mounted, so the webview holds nothing of them, and their requests wait while pages on screen render, as thumbnails do. One row each way and no more: each page rendered ahead takes its place in the image cache and in MuPDF's store from pages already seen.
- An image is never rendered twice at once (`Renders` in `protocol.rs`): a second request for it waits for the first and takes its result from the cache. The first tile of a page at a scale renders alone, since it decodes the page's images, which MuPDF then keeps for the other tiles.

**Text geometry.** For text selection and highlights, Rust extracts MuPDF's structured text per page (characters with their quads) and sends it to the frontend once per page and revision. The frontend does hit-testing and selection locally against that cached geometry, then sends the selected character range back with the highlight operation, so Rust computes the final quads. A page's links (`pdf-core/src/links.rs`: where each is, and its target resolved as a bookmark's is) come with its text, and the frontend finds the one under the pointer the same way.

**Windows.** Every window is the whole app with tabs of its own (section 8): the first is `main`, later ones `window-2`, `window-3`… All share the one process, its documents and its app data. `src-tauri/src/windows.rs` keeps which window shows which document; moving a tab to another window changes only that (`move_document`), the session with its unsaved changes and undo history staying as it is, and the window that gets the tab asks for it (`list_open_documents`). Rust sends an event to the window it concerns (`emit_to`), and the frontend listens through its own window (`listen` in `src/lib/ipc/index.ts`), since a plain listener would hear every window's events. A window's documents close when it is destroyed; Lectrix quits with its last window.

**IPC surface.** Commands live in `src-tauri/src/commands.rs`, all typed, among them `get_document_info` (page count, page sizes and rotation, outline, page labels, annotation list), `get_page_text`, `search_text`, `apply_operation`, `undo`, `redo`, `save`, `save_as`, `plan_merge`, `execute_merge`, `extract_pages`, `scan_annotations_for_repair` and `repair_annotations`. Every mutating command returns the new document revision plus the changed data, so the frontend never re-fetches everything. The webview can call only the commands it is granted: a new one is declared in `src-tauri/build.rs` and granted in `src-tauri/capabilities/default.json` (a test checks both), or calling it fails at run time.

## 4. Repository layout

A Cargo workspace plus a SvelteKit app at the root. Keep this structure unless an ADR justifies a change.

```
lectrix/
├─ AGENTS.md                 # this document
├─ README.md                 # the project's page on GitHub, for the people who use Lectrix
├─ CONTRIBUTING.md           # building, testing and the rules in short, for contributors
├─ CHANGELOG.md              # what changed in each version, for users (docs/releasing.md)
├─ LICENSE                   # AGPL-3.0-or-later
├─ .github/                  # CI, soak and release workflows (ADR 0011)
├─ THIRD_PARTY_NOTICES.md
├─ THIRD_PARTY_LICENSES.md   # generated by tests/licenses/notices.py
├─ Cargo.toml                # workspace
├─ crates/
│  ├─ pdf-core/              # all PDF logic, no Tauri dependency
│  │  ├─ src/
│  │  │  ├─ session.rs       # document actor, revisions, recovery copies
│  │  │  ├─ render.rs        # display lists, tiles
│  │  │  ├─ text.rs          # structured text, search
│  │  │  ├─ geometry.rs      # view <-> PDF user space transforms
│  │  │  ├─ annot/           # write profile, appearance streams, repair
│  │  │  ├─ outline/         # bookmarks
│  │  │  ├─ labels.rs        # page labels
│  │  │  ├─ merge/           # stitching, and extracting pages into a new file
│  │  │  ├─ pages.rs         # moving and deleting pages (ADR 0018)
│  │  │  ├─ save.rs          # incremental / full / atomic replace
│  │  │  ├─ ops.rs           # operation enum, undo/redo
│  │  │  ├─ fonts/           # installed-font index (ADR 0005)
│  │  │  ├─ platform/        # platform traits and their Windows implementations
│  │  │  ├─ testgen.rs       # generated sample documents
│  │  │  └─ ffi/             # thin safe wrappers over mupdf-sys, plus shim.c
│  │  └─ tests/
│  └─ pdf-cli/               # headless CLI over pdf-core (tests, debugging, corpus survey)
├─ src-tauri/                # Tauri app crate; windows/ holds the installer changes (ADR 0007)
├─ src/                      # SvelteKit frontend
│  ├─ lib/components/
│  ├─ lib/features/          # viewer/, bookmarks/, labels/, merge/, organize/, annotations/, recovery/
│  ├─ lib/ipc/               # generated types + typed invoke wrappers
│  ├─ lib/stores/
│  └─ routes/
├─ third_party/              # vendored mupdf crate (LECTRIX_PATCHES.md) and MuPDF headers
├─ design/                   # brand kit sources (icons, wordmark, file icon, tokens); not shipped (ADR 0013)
├─ tests/
│  ├─ corpus/                # real-world PDFs (Git LFS), read-only
│  ├─ local-corpus/          # scripts for the user's private corpus (files git-ignored)
│  ├─ interop/               # cross-renderer harness
│  ├─ e2e/                   # tauri-driver + WebdriverIO
│  ├─ installer/             # silent install/uninstall check (CI only)
│  ├─ licenses/              # dependency license check
│  ├─ perf/                  # performance measurements
│  └─ soak/                  # the tests many times over, to find the ones that fail only sometimes
└─ docs/
   ├─ decisions/             # ADRs: NNNN-title.md
   ├─ spec/                  # what each feature does, one file per feature (section 6), and UI details (section 8)
   ├─ upstream/              # patches to offer upstream projects
   ├─ interop-profile.md     # the annotation write profile, kept in sync with section 5
   ├─ manual-checklist.md    # the manual release checklist (section 9)
   ├─ releasing.md           # the updater key and how to publish a release
   ├─ versions.md
   └─ status.md              # measurements, known gaps, platform gaps
```

The `pdf-cli` tool exposes every `pdf-core` operation from the command line (for example `pdf-cli labels set in.pdf out.pdf --rule 0:roman-lower --rule 12:decimal`). Tests and the interop harness use it, and it makes engine bugs reproducible without the UI.

## 5. Annotation interoperability contract

Every annotation this app writes must display, print and be editable in Acrobat Reader, PDFium-based viewers (Edge, Chrome), pdf.js (Firefox) and Foxit. Write conservatively, read leniently, and never weaken these rules to make a test pass: report the conflict instead. Mirror this section in `docs/interop-profile.md`, which also records where each rule is enforced and tested.

### 5.1 Write profile (mandatory for every annotation written or edited)

1. **Standard subtypes only:** Highlight, Underline, StrikeOut, Squiggly, Text (sticky note), Ink, FreeText. No custom subtypes and no private keys.
2. **Appearance stream always.** Every annotation gets a normal appearance (`/AP /N`), regenerated after every edit, using MuPDF's appearance synthesis with the corrections in ADR 0006. If MuPDF's appearance for a type fails the interop tests (section 9), write a custom appearance stream for that type in `annot/` and document why in an ADR.
3. **QuadPoints in Acrobat order.** For text-markup types, each quad is written as upper-left, upper-right, lower-left, lower-right, in PDF user space. This is the de facto order Acrobat uses, not the counter-clockwise order the spec text describes. One function writes quads, with unit tests. Upper and lower, left and right are as the glyphs stand, in right-to-left text too (ADR 0017).
4. **Rect contains everything.** `/Rect` is the union of all quads, ink paths, or the text box, plus the stroke width and a 1 pt margin.
5. **Highlights blend.** Highlight appearance streams use an ExtGState with Multiply blend mode so text stays readable. Store opacity in `/CA` on the annotation as well as in the appearance.
6. **Complete metadata:** `/NM` (a UUID), `/T` (author, from settings, defaulting to the Windows user name), `/CreationDate` and `/M` (PDF date strings with time zone), `/F 4` (Print flag), `/C` (color, as the type defines it: for FreeText `/C` is the background, so text boxes get `/C []` and their text color in `/DA`), `/P` (page reference). Sticky notes and markup with a note get a linked `/Popup` annotation with the `/Parent` back-reference.
7. **Plain FreeText.** Use the base-14 Helvetica font through `/DA`, one font size and one color per box, plain-text `/Contents`, and an appearance stream. Do not write `/RC` rich text.
8. **Correct coordinates.** All conversions between screen and PDF user space go through `geometry.rs`, which handles `/Rotate` (0, 90, 180, 270), a CropBox whose origin is not (0, 0), and `/UserUnit`. Test each case with fixtures.
9. **Lean ink.** Simplify freehand strokes (Ramer–Douglas–Peucker, about 0.5 pt tolerance) before writing.
10. **Leave others' work alone.** Never rewrite, reorder or drop annotations the user did not touch. When the user edits an annotation made by another app, change only the edited keys plus its appearance, and keep unknown keys. Text markup whose QuadPoints are not in Acrobat order gets them put in it by that edit (the same corners), since the new appearance is drawn from them.

### 5.2 Reading other apps' annotations

- Display every standard annotation type MuPDF supports, including ones this app cannot create (shapes, stamps, links, file attachments).
- If an annotation lacks an appearance stream or has malformed quads, draw it for display from its properties, but write nothing. Mark it in the annotation list with a "needs repair" badge.
- QuadPoints out of Acrobat order need repair only when their corners are crossed (the spec's counter-clockwise order, a clockwise one), which readers that draw from them show as a bow-tie. Mirrored ones (the bottom edge first, as apps built on Apple's PDFKit write, or each edge from right to left) are the same shape read either way: with an appearance, the annotation shows correctly everywhere and nothing is reported (ADR 0017).
- Show replies (`/IRT`) in their parent's thread in the list. Replies Lectrix writes are notes linked by `/IRT` that nothing draws on the page: an empty appearance, a `/Rect` of no area at the parent's corner, no `/Popup` (ADR 0012; `docs/interop-profile.md`, "Replies"). Only their text and author change.

### 5.3 Repair annotations command

A command that scans the document, shows a summary of problems found (counts per problem type), and on confirmation fixes them in one undoable operation:

- generate missing appearance streams;
- reorder crossed QuadPoints into Acrobat order; mirrored ones stay as their app wrote them while the annotation keeps its appearance, and are put in order only where repair has to draw the annotation (ADR 0017);
- recompute `/Rect` where it does not contain the content;
- add missing `/NM`, `/F 4`, `/M`, and `/P`.

Repair must never change an annotation's content, color, author or position. Log every change to the app log with the annotation's object number.

### 5.4 Saving rules that protect interoperability

- Default save is incremental: changes are appended and the original bytes stay intact.
- If MuPDF reports the file was repaired on open (damaged cross-reference table), it cannot be saved incrementally; do a full save and tell the user once, in plain words, why. A file that opens without an error but with no pages (a cross-reference table that leads nowhere, such as a last section naming itself as the previous one) gets the same repair from Lectrix, and is then treated the same way; if it still has no pages, it is reported as too damaged to open.
- "Save As (optimized)" does a full rewrite with garbage collection and stream compression.
- If the document is digitally signed, warn before the first edit that edits will be shown as changes after signing, and always save incrementally.
- Encrypted documents: prompt for the password, keep the encryption on save, and respect permission flags (no annotating if the document disallows it).
- PDF/A: a file that claims a PDF/A level (`pdfaid:part` in the XMP metadata of the catalog's `/Metadata`; the claim is read, compliance is not checked) opens for reading only, as Acrobat opens it. The session refuses every operation meanwhile, the frontend offers nothing that changes the document (annotations, bookmarks, page labels, rotating, moving, deleting or inserting pages, repair; such a shortcut says why in a notification with an Enable editing button), and bookmarks expanded or collapsed meanwhile are not written. A banner under the view bar says so, with the level ("This file claims compliance with the PDF/A standard (PDF/A-1b), so it was opened for reading only. Saving annotations or inserted pages removes that claim."), and its **Enable editing** button makes the document one like any other for as long as it is open. Settings can open such files ready to edit (section 6.6), and one that comes with changes restored after a crash opens that way. Lectrix cannot vouch that what it writes complies, so a file never keeps a claim Lectrix may have broken: a save whose changes include an annotation added, changed, deleted or repaired, pages inserted from another file, or pages deleted, takes the identification properties (`pdfaid:part`, `conformance`, `amd`, `corr`, `rev`) out of the XMP, leaves the rest of the metadata as it was, and says so in a notification ("report.pdf no longer claims to be PDF/A-1b."). Changes undone before the save do not count. Bookmarks, page labels, page rotation and the order of the pages alone keep the claim, as does a Save As, optimized or not. A document restored after a crash loses the claim at its next save, since what its changes were is not known.

## 6. Feature specifications

What each feature does is specified in `docs/spec/`, one file per feature, under the section numbers below, so a reference such as "section 6.5" leads to its file. Read a feature's spec before changing it, and change the spec in the same commit as its behavior. The "settled details" in each were decided during the v1 build; change them only with the user's agreement.

- **6.1 Viewer** ([docs/spec/viewer.md](docs/spec/viewer.md)): opening (dialog, drag-and-drop, file association, command line; single instance), tabs spread over windows, continuous virtualized scrolling, zoom, navigation by page number or label with back/forward history, text selection and a text cursor, search with suggestions and scrollbar marks, links, four scroll modes, view rotation, full screen, visual adjustments, the Pages panel, the start screen and recent files. View rotation, pages turned on screen and visual adjustments are display only: never written to the file, remembered per file in app data. A web link is opened by Rust (`open_link`) only if a link on that page carries it, and nothing but an http or https address is ever handed to the system.
- **6.2 Bookmarks** ([docs/spec/bookmarks.md](docs/spec/bookmarks.md)): the outline as a tree in a side panel: add (Ctrl+B), rename, drag to reorder and nest, delete, retarget, a title colour, and an inspector beside the row. New and retargeted bookmarks get explicit `/XYZ left top null` destinations; bookmarks no edit touched keep their bytes; non-ASCII titles are UTF-16BE with a byte-order mark. Expanding or collapsing is written at the next save but is not an edit.
- **6.3 Page labels** ([docs/spec/page-labels.md](docs/spec/page-labels.md)): set only in a dialog over the Organize pages grid, for all pages, the selected ones or a range, with a live preview, in one undo step. Labels are shown exactly as stored, letter styles go a…z then aa…zz, and a document numbered 1, 2, 3 throughout has no `/PageLabels`.
- **6.4 Stitching** ([docs/spec/combine.md](docs/spec/combine.md)): a Combine tab with the files in a pane and all their pages in one grid (`PageGrid.svelte`, shared with 6.7) to reorder, rotate and remove pages, with options for bookmarks and labels. Links and destinations are remapped and colliding names prefixed; the result is always a new file and the sources are never modified. Insert pages from file uses the same engine.
- **6.5 Annotations** ([docs/spec/annotations.md](docs/spec/annotations.md)): the tools, one colour for every tool with each tool's opacity, stroke width and font size, the colour panel, quick tools on selected text, the annotation bar and the inspector, the annotation list (search, sort, filters, replies), comment tooltips, and annotating turned pages. Every change is one undo step.
- **6.6 Settings, About and installers** ([docs/spec/settings.md](docs/spec/settings.md)): the Settings dialog, About, updates, What's new, releases and the installers. Settings rows are declared in `src/lib/features/settings/schema.ts` and commands with shortcuts in `src/lib/shortcuts.ts`: a new setting is a field in Rust's `Settings` plus a row there, a new command with a shortcut an entry in `SHORTCUT_GROUPS`, and only what differs from the defaults is stored.
- **6.7 Organize pages** ([docs/spec/organize.md](docs/spec/organize.md)): a mode of the document's tab that shows its pages as a grid to move, rotate (`/Rotate`), delete, insert from files, extract into a new document and label. Each change is made in the document at once as one undo step (ADR 0018), and what is kept by page follows its page (`DocumentChange.pageMap`).

## 7. Document model, undo/redo and saving

The Rust session is the single source of truth: the frontend holds only a view of it, and the undo history lives in Rust.

**Operations.** Every change is a variant of one `Operation` enum in `ops.rs` (for example `AddAnnotation`, `UpdateAnnotation`, `DeleteAnnotation`, `AddBookmark`, `SetPageLabels`, `RotatePages`, `MovePages`, `DeletePages`, `InsertPages`, `RepairAnnotations`). Applying an operation increments the document revision and returns what changed. The session reads again only what the operation can have changed: the whole outline, which takes a tenth of a second in a file with thousands of bookmarks, only after bookmark edits and after pages are deleted or inserted (`OutlineEffect` in `ops.rs`, ADR 0018).

**Undo/redo.** Uses MuPDF's built-in journalling, wrapped in `ffi/journal.rs`; each `Operation` is one journal step with a human-readable name ("Add highlight", "Rename bookmark"). The Edit menu shows the name of the step being undone or redone. Saving reopens the file, so undo history starts again after each save (ADR 0003); after a crash restore it starts at the restored state (MuPDF cannot load a saved journal, ADR 0001).

**Dirty state.** A document is dirty when its revision differs from the last saved revision. Show a dot in the tab (a spinning ring while saving; there is no status bar), and prompt when the tab closes, when its window closes and on app exit.

**Documents without a file** (ADR 0019). Pages extracted into a new document (section 6.7) open as a document that is no file yet. Its tab has the name it would be saved under ("Pages from report.pdf"), the unsaved dot from the start, and "Not saved yet" as its tooltip. Save asks where to save it, as Save As does, starting in the folder of the document its pages came from; saved, it is that file and a document like any other. Closing its tab, its window or the app asks first ("Pages from report.pdf has not been saved yet. Do you want to save it?": Save, Don't save or Cancel). Until it is saved it is not among the recent files, has no remembered view, is not watched and has nothing to reload; undoing every change leaves it unsaved. Its pages are in a working copy in the app's local data folder (`unsaved/`, beside the recovery copies), which goes when the document is saved or closed, and at the next start if Lectrix did not end normally. Crash recovery covers it as any document with unsaved changes, and restores it as a document without a file.

**Atomic save.**

1. Write the new file to a temporary file in the same folder as the target.
2. Flush it to disk.
3. Replace the original in one rename on the same volume.
4. On failure, delete the temp file and leave the original untouched.

A failed save also leaves the document in memory as it was, so saving again (or Save As) is safe: an incremental save is written without moving the document on to the update it wrote, and "Save As (optimized)" optimizes a copy of the document (ADR 0003, "A save that fails").

Documents are opened through a file stream that allows that rename (ADR 0003). On Windows the rename is retried for up to a second while a file is locked, since antivirus scanners and the search indexer open a file just written for a moment. If the target is still locked by another program (Acrobat locks files it has open), show a clear message naming the likely cause and offer Save As.

**Crash recovery.** Every 2 minutes, each dirty document whose content changed since its last copy gets a recovery copy in the app's local data folder (a snapshot with the unsaved changes as an incremental update; damaged files are copied in full). Saving, reloading or closing a document deletes its copy, and so does quitting normally. On the next launch, before opening startup files, Lectrix offers to restore copies a crash left behind: Restore, Discard (asks once more) or Not now (asked again next time; Escape means Not now). A restored document opens in a tab for its own file, marked unsaved, and saves incrementally onto it. Encrypted files ask for their password again.

**External changes.** Watch open files. If a file changes on disk and the document has no unsaved edits, offer to reload; if it has unsaved edits, warn and offer Save As.

## 8. UI and UX guidelines

The app should feel like a native Windows 11 app: calm, fast, and keyboard-friendly, with the document always the visual focus. The layout, the controls and their motion are described in full in [docs/spec/ui.md](docs/spec/ui.md) (sections 8.1 to 8.4).

**Layout** (8.1). A custom title bar holds the app's icon and name (a button that shows the start screen), the menus (on macOS, the system's menu bar), the document tabs and the window buttons, drawn as each platform draws them (ADR 0021). Every window is the whole app, and windows exchange tabs (ADR 0016). Below the title bar, a view bar at the top of the document holds the page and zoom controls; side panes hold three panels (Pages, Bookmarks, Annotations) that move freely between them; the annotation toolbar is docked or floating (Settings). Every pane beside the pages resizes from its inner edge (`PaneResizer.svelte`), and its width is remembered in app data.

**Visual style.**

- Colours from Neo, the Obsidian theme: its light theme and its Midnight dark theme, with the greys tinted to Lectrix blue's hue, solid throughout (no Mica; ADR 0014).
- Follow the system light/dark setting by default; Settings can force light or dark. The accent is Lectrix blue from the brand kit (ADR 0010): `#2F5DAA` in the light theme, `#6FA3EF` in the dark one.
- As in Neo: document tabs are pills, the active one raised; menus, popovers and dropdown lists are flat, with a thin edge and no shadow; notifications have a slight shadow; clickable controls show the hand cursor.
- Controls are the app's own, never the system's (8.2): dropdowns (`Dropdown.svelte`, never `<select>`), sliders (`Slider.svelte`) and tooltips (`Tooltips.svelte`: a control names its tooltip in `data-tip`, never in `title`, and a test checks).
- Font stack: Google Sans, bundled with the app (ADR 0009), then Segoe UI Variable, Segoe UI and the system UI fonts for characters it lacks.
- Spacing on an 8 px grid (4 px for tight spots); corner radius 6–8 px; thin borders instead of heavy shadows.
- All colors, sizes and radii as CSS variables (design tokens) consumed by Tailwind; no hard-coded colors in components.

**Behavior.**

- Never block the UI thread. Long operations (combining, repair, saving a large file) show progress and can be canceled.
- Placeholder page shapes at the correct size while pages render, so the scroll position never jumps.
- Errors in plain language with a suggested next step; never show raw error text or crash on bad input. Log details to a rotating log file.
- Respect reduced-motion settings; keep animations under 150 ms. Notifications, dialogs (sheets on macOS) and progress bars each move in one way, in one place (8.3).

**Accessibility.** Every control is reachable by keyboard with a visible focus ring (the page canvas, which takes focus to scroll from the keyboard, has none: a ring around the document is noise; nor have What's new's list of changes, which takes the focus as the dialog opens for the same reason, and the About dialog, which takes it itself), has an accessible name, and meets WCAG AA contrast (checked by `src/lib/contrast.test.ts` in both themes). Shift+F10 and the Menu key open the focused control's context menu. App-wide shortcuts such as Ctrl+S work while typing in panel fields.

**Default shortcuts** (the commands' ones can be changed in Settings > Shortcuts, section 6.6; the keys for what has the focus, such as Esc, Del, F2 and the arrow-key combinations, are fixed; keyboards with non-Latin letters and macOS: 8.4):

| Action | Shortcut |
| --- | --- |
| Open / Save / Save As | Ctrl+O / Ctrl+S / Ctrl+Shift+S |
| Undo / Redo | Ctrl+Z / Ctrl+Y |
| Find | Ctrl+F |
| Add bookmark | Ctrl+B |
| Zoom in / out / fit width | Ctrl+= / Ctrl+- / Ctrl+0 |
| Full screen | F11 |
| In full screen, show the side panes and the toolbar | Alt (held) |
| Go to page | Ctrl+G |
| Next / previous tab | Ctrl+Tab / Ctrl+Shift+Tab |
| Select / Highlight / Underline / Note / Pen / Text box tool | Esc / H / U / N / P / T |
| Rename bookmark | F2 |
| Delete selection | Del |
| Move the text cursor | Arrows (with Ctrl, by a word / a paragraph) |
| Extend or reduce selected text by a character / a line | Shift+Left, Right / Shift+Up, Down |
| Extend or reduce selected text by a word / a paragraph | Ctrl+Shift+Left, Right / Ctrl+Shift+Up, Down |
| Color for new annotations: preset 1 to 9 (yellow, green, red, blue, orange, purple, light yellow, light green, pink) | 1 to 9 |
| Settings | Ctrl+, |
| Context menu | Shift+F10 / Menu key |

## 9. Testing strategy and interop harness

Interoperability is verified by machines on every commit and by a person before every release; neither replaces the other.

**Test corpus** (`tests/corpus/`, Git LFS, never modified by tests). Collect at least one file per category:

- rotated pages (90, 180, 270) and a CropBox with a non-zero origin;
- a damaged cross-reference table, an encrypted file, a signed file;
- a large file (1,000+ pages) and a scanned file with no text layer;
- CJK and right-to-left text;
- files with existing outlines, existing page labels, and form fields;
- annotations made by other apps, especially the ones that display wrongly in Acrobat (the user will supply these).

The corpus is still empty (`docs/status.md`); the geometric cases come from generated files (`pdf-core/src/testgen.rs`). Write test outputs to `target/test-output/`, never next to corpus files.

Real-world files the user owns but cannot publish (for example books from their Calibre library) form a **local corpus**: `tests/local-corpus/survey.py` profiles a library read-only, `select.py` copies one or two files per category into `tests/local-corpus/files/` and writes `manifest.json` (both git-ignored), and `crates/pdf-core/tests/local_corpus.rs` runs over them, skipping when they are absent (as in CI). The source library is only ever read, never written.

**Unit tests (`pdf-core`):** coordinate transforms for every rotation and CropBox case; quad ordering; label formatting (roman, letters past z, prefixes); outline serialization including Unicode titles; atomic save failure paths.

**Round-trip tests:** for each annotation type, create → save → reopen with MuPDF → assert every key the write profile requires, with the expected values. Same for outlines, labels and merges.

**Structural check:** run `qpdf --check` on every file the tests produce; any warning fails the test.

**Cross-renderer harness (`tests/interop/`):**

1. For each test output, render each annotated page with three independent engines: MuPDF (`mutool draw`), PDFium (through a small script using the `pypdfium2` package), and pdf.js (a Node script using `pdfjs-dist`).
2. Inside each annotation's `/Rect`, compare the render against the same page rendered without annotations. If any engine shows no visible change, the test fails: that annotation is invisible in that engine.
3. Compare engines against each other with a tolerance, and save diff images for any failure.

The suites keep the names they got during the build: `phase0` (labels, outline, merge and highlight from `pdf-cli`), `phase2` (bookmarks), `phase3` (labels), `phase4` and `phase4-local` (combining), `pages` (moving and deleting pages, named for what it checks), `phase5` and `phase5-local` (annotations and repair). SumatraPDF and similar apps are built on MuPDF and do not count as an independent check.

**Manual release checklist** (`docs/manual-checklist.md`, performed by the user before every release; the agent prepares the files and keeps the checklist current): open the interop sample files in Acrobat Reader, Edge, Firefox and Foxit; for every annotation type confirm it is visible, has the right color and opacity, appears in the comments list, prints, and can be edited and deleted. Then the installers, a real crash, and a Narrator walk.

**Frontend and end-to-end:** Vitest for stores and component logic; `tauri-driver` with WebdriverIO for the critical flows (open, highlight, add bookmark, set labels, combine, save, reopen, crash recovery, recent files, tabs and windows, full screen, organizing pages, and a keyboard-only accessibility walk). The end-to-end tests run beside a Lectrix the user has open and leave it alone: a test run (`LECTRIX_EPHEMERAL`) is an instance of its own, outside the single-instance hand-off, and the launcher ends only copies of the app under test (`tests/e2e/lib/app.mjs`). **Run them as CI shows the app before pushing** (`npm run test:ci --prefix tests/e2e`, one or more test files after `--`): CI's Windows runners ask for reduced motion and have a 1024 x 768 screen, where the window's page is 1028 px wide and the view bar wraps sooner, and a test written on a larger screen with animations on can fail there alone. The mode passes the browser its reduced-motion switch (`LECTRIX_BROWSER_ARGS`) and gives a window without a remembered place that size (`LECTRIX_WINDOW_SIZE`). A test of motion checks the way there only where motion is allowed (`matchMedia('(prefers-reduced-motion: reduce)')`), and where things end up either way.

**Debug menu** (development builds, or a release build run with `LECTRIX_DEBUG=1`; `LECTRIX_DEBUG=0` hides it): after Help, it shows each notification, notice and prompt on demand, worded as the app words them: the update notice at every stage, the note after an update (start screen or notification), information and error notifications, the notification that a save removed a PDF/A claim, the file-changed banner, the banner of a PDF/A file opened for reading only (the open document can't be changed until Enable editing or Clear all) and the crash recovery offer, plus Clear all. Rehearsals change nothing in Rust or on disk: the update notice's buttons work on a stand-in that pretends to download and to restart. The rehearsed update brings what `CHANGELOG.md` lists under Unreleased, so the next release's notes can be read as users will see them. A new kind of notification gets an item there (`src/lib/features/debug/`).

**CI** (GitHub Actions): on `windows-latest`, five jobs. `checks`: `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test`, `svelte-check`, Vitest and the license check. `build`: the release app, its installer (uploaded as an artifact), `pdf-cli` and `tauri-driver`, once. On that build, side by side: `interop` (the interop harness), `installer` (a silent install/uninstall check of the installer, which also has it take over an install made by the 1.1.0 MSI) and `e2e` (the end-to-end tests). Every job, and every step of `checks`, runs whatever the others do, so one run reports every failure, and a job that failed is run again by itself (Re-run failed jobs), not with the rest. macOS and Linux jobs build the app and its tests and run the tests without failing the job. After CI passes on `main`, the Release workflow publishes a version that has no release yet (ADR 0011).

**Soak run** (`.github/workflows/soak.yml`, `tests/soak/run.mjs`): the end-to-end tests and the Rust tests ten times over, each pass to its end, then one list of every test that failed in any pass, with how often and why, on the run's page. It finds the tests that fail only some of the time, which a CI run shows one at a time, on the day each happens to fail. Started by hand (Actions > Soak), and every night when `main` changed that day; it gates nothing. Run it before a release, fix what it lists in one commit, and run it again. Locally: `node tests/soak/run.mjs --passes 3 --like-ci`.

## 10. Working rules for the agent

Follow these rules in every session; when a rule and a convenience conflict, the rule wins.

**Process.**

- Start each session by reading this document and `docs/status.md`. Before changing a feature, read its spec in `docs/spec/` (section 6), and `docs/spec/ui.md` before changing the layout or a shared control.
- Work on one change at a time. For a new feature or a larger change, propose a plan and wait for the user's go-ahead before building.
- When a change is done and CI is green, report to the user: what was built, the tests, any measurements that changed, behavior choices made without explicit guidance, and decisions needed. Then stop for review.
- Add every change users will notice to `CHANGELOG.md`, under `## Unreleased`, in the commit that makes it, worded as `docs/releasing.md` says.
- Keep the docs current: `docs/status.md` (add gaps you find, remove gaps you close), `docs/versions.md`, `docs/interop-profile.md`, `docs/manual-checklist.md`, and, when the product's behavior changes, the spec in `docs/spec/` that describes it (this document only for what it covers: scope, constraints, architecture, the interop contract, saving, testing and these rules). Keep this document short: a feature's details go in its spec, not here.
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
