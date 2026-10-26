# Architecture and repository layout

Part of the Lectrix specification. [AGENTS.md](../AGENTS.md) holds the rules every change follows, with sections 3 and 4 in short; this file is sections 3 and 4 in full, and keeps its numbers, so a reference to it still leads here.

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

- The frontend requests pages through a custom URI protocol: `lectrix://page/{docId}/{pageIndex}?scale={s}&rev={r}`. Including the document revision in the URL makes cache invalidation automatic. `&hide={n},{n}…` names annotations to leave out of the image, by object number: the ones the annotation list filters out (section 6.5). `&layer=content`, `annots` or `annots-dark` asks for the page without its annotations or for them alone, for visual adjustments that leave annotations out (section 6.1).
- Rust renders the page with MuPDF and returns a PNG at the fastest compression level (ADR 0004). Raw RGBA drawn into a `<canvas>` stays available for measurement (`LECTRIX_IMAGE_FORMAT`).
- Above a zoom threshold, render 512 px tiles instead of whole pages.
- The start screen's grid gets recent files' first pages from `lectrix://recent/{index}?w={px}&opened={ms}`: rendered from the file on disk without opening it as a document, named by the file's place in the recent list and the time it was opened (never by a path from the webview), and kept in memory while the file's size and modification time are unchanged.
- Keep an LRU cache of rendered images keyed by document, page, scale bucket and revision, with a configurable memory cap.
- Mount only pages near the viewport (a quarter screen above and below, ADR 0002); show a sized placeholder for the rest. Thumbnails wait while pages on screen render.
- Render ahead: once the view has rested for 120 ms, the frontend has Rust render the row of pages after the mounted ones and the row before them into the image cache (the page URL with `&warm=1`, which answers with no image), so a page scrolled or turned to is there already; a scanned page takes a quarter of a second to render. These pages are not mounted, so the webview holds nothing of them, and their requests wait while pages on screen render, as thumbnails do. One row each way and no more: each page rendered ahead takes its place in the image cache and in MuPDF's store from pages already seen.
- An image is never rendered twice at once (`Renders` in `protocol.rs`): a second request for it waits for the first and takes its result from the cache. The first tile of a page at a scale renders alone, since it decodes the page's images, which MuPDF then keeps for the other tiles.

**Text geometry.** For text selection and highlights, Rust extracts MuPDF's structured text per page (characters with their quads) and sends it to the frontend once per page and revision. The frontend does hit-testing and selection locally against that cached geometry, then sends the selected character range back with the highlight operation, so Rust computes the final quads. A page's links (`pdf-core/src/links.rs`: where each is, and its target resolved as a bookmark's is) come with its text, and the frontend finds the one under the pointer the same way.

**Documents and views.** A document open in a window is one `DocModel` in the frontend (what Rust reported: pages, annotations, outline, labels, revision, text cache), shown by one or more views (`DocTab`: zoom, place, rotation, selection, search, history), so the same document can be shown side by side (section 6.1, split view, ADR 0023). A tab of the row is a `TabGroup` of one to four views. Rust knows nothing of views: a document has one session however often it is shown.

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
│  │  │  ├─ compress/        # Compress: images, fonts, removals, duplicates (ADR 0027)
│  │  │  ├─ print/           # printing: pages on sheets, bands for printers, the PDF for CUPS (ADR 0028)
│  │  │  ├─ save.rs          # incremental / full / atomic replace
│  │  │  ├─ ops.rs           # operation enum, undo/redo
│  │  │  ├─ fonts/           # installed-font index (ADR 0005)
│  │  │  ├─ platform/        # platform traits (fonts, printers) and their implementations
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
   ├─ status.md              # where things stand: an index, one line per gap
   └─ status/                # measurements, known gaps, platform gaps, releases, planned tools
```

The `pdf-cli` tool exposes every `pdf-core` operation from the command line (for example `pdf-cli labels set in.pdf out.pdf --rule 0:roman-lower --rule 12:decimal`). Tests and the interop harness use it, and it makes engine bugs reproducible without the UI.
