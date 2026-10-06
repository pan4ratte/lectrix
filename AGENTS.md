# Lectrix — Instructions for Coding Agents

Oct 4, 2026 · @Mark

## 1. Mission and scope

Lectrix is a lightweight, open-source PDF editor for Windows, designed to go cross-platform later, whose annotations, bookmarks and page labels open correctly in Acrobat and every other mainstream reader. Interoperability is the product's core promise: a feature that works only inside this app is a failed feature.

This document is the single source of truth for the product and for how to work on it. Read it in full at the start of every session. v1 is complete (2026-10-04); `docs/status.md` lists its measurements and known gaps. The name is **Lectrix**; the frontend keeps it in one config constant (`APP_NAME` in `src/lib/config.ts`).

**v1 contains:**

- **Viewer:** fast continuous scrolling, zoom, thumbnails, text selection, text search, tabs for multiple documents.
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
| App shell | Tauri 2 | WebView2 on Windows. Bundled with the Tauri bundler (NSIS and MSI). |
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
- **Network:** none at runtime except the update check and download from Lectrix's GitHub releases (ADR 0011), which Settings can turn off. No telemetry, and the webview never goes online.
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

- The frontend requests pages through a custom URI protocol: `lectrix://page/{docId}/{pageIndex}?scale={s}&rev={r}`. Including the document revision in the URL makes cache invalidation automatic.
- Rust renders the page with MuPDF and returns a PNG at the fastest compression level (ADR 0004). Raw RGBA drawn into a `<canvas>` stays available for measurement (`LECTRIX_IMAGE_FORMAT`).
- Above a zoom threshold, render 512 px tiles instead of whole pages.
- Keep an LRU cache of rendered images keyed by document, page, scale bucket and revision, with a configurable memory cap.
- Mount only pages near the viewport (a quarter screen above and below, ADR 0002); show a sized placeholder for the rest. Thumbnails wait while pages on screen render.

**Text geometry.** For text selection and highlights, Rust extracts MuPDF's structured text per page (characters with their quads) and sends it to the frontend once per page and revision. The frontend does hit-testing and selection locally against that cached geometry, then sends the selected character range back with the highlight operation, so Rust computes the final quads.

**IPC surface.** Commands live in `src-tauri/src/commands.rs`, all typed, among them `get_document_info` (page count, page sizes and rotation, outline, page labels, annotation list), `get_page_text`, `search_text`, `apply_operation`, `undo`, `redo`, `save`, `save_as`, `plan_merge`, `execute_merge`, `scan_annotations_for_repair` and `repair_annotations`. Every mutating command returns the new document revision plus the changed data, so the frontend never re-fetches everything.

## 4. Repository layout

A Cargo workspace plus a SvelteKit app at the root. Keep this structure unless an ADR justifies a change.

```
lectrix/
├─ AGENTS.md                 # this document
├─ LICENSE                   # AGPL-3.0-or-later
├─ .github/                  # CI and release workflows (ADR 0011)
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
│  │  │  ├─ merge/           # stitching
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
│  ├─ lib/features/          # viewer/, bookmarks/, labels/, merge/, annotations/, recovery/
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
│  └─ perf/                  # performance measurements
└─ docs/
   ├─ decisions/             # ADRs: NNNN-title.md
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
3. **QuadPoints in Acrobat order.** For text-markup types, each quad is written as upper-left, upper-right, lower-left, lower-right, in PDF user space. This is the de facto order Acrobat uses, not the counter-clockwise order the spec text describes. One function writes quads, with unit tests.
4. **Rect contains everything.** `/Rect` is the union of all quads, ink paths, or the text box, plus the stroke width and a 1 pt margin.
5. **Highlights blend.** Highlight appearance streams use an ExtGState with Multiply blend mode so text stays readable. Store opacity in `/CA` on the annotation as well as in the appearance.
6. **Complete metadata:** `/NM` (a UUID), `/T` (author, from settings, defaulting to the Windows user name), `/CreationDate` and `/M` (PDF date strings with time zone), `/F 4` (Print flag), `/C` (color, as the type defines it: for FreeText `/C` is the background, so text boxes get `/C []` and their text color in `/DA`), `/P` (page reference). Sticky notes and markup with a note get a linked `/Popup` annotation with the `/Parent` back-reference.
7. **Plain FreeText.** Use the base-14 Helvetica font through `/DA`, one font size and one color per box, plain-text `/Contents`, and an appearance stream. Do not write `/RC` rich text.
8. **Correct coordinates.** All conversions between screen and PDF user space go through `geometry.rs`, which handles `/Rotate` (0, 90, 180, 270), a CropBox whose origin is not (0, 0), and `/UserUnit`. Test each case with fixtures.
9. **Lean ink.** Simplify freehand strokes (Ramer–Douglas–Peucker, about 0.5 pt tolerance) before writing.
10. **Leave others' work alone.** Never rewrite, reorder or drop annotations the user did not touch. When the user edits an annotation made by another app, change only the edited keys plus its appearance, and keep unknown keys.

### 5.2 Reading other apps' annotations

- Display every standard annotation type MuPDF supports, including ones this app cannot create (shapes, stamps, links, file attachments).
- If an annotation lacks an appearance stream or has malformed quads, draw it for display from its properties, but write nothing. Mark it in the annotation list with a "needs repair" badge.
- Show replies (`/IRT`) in their parent's thread in the list. Replies Lectrix writes are notes linked by `/IRT` that nothing draws on the page: an empty appearance, a `/Rect` of no area at the parent's corner, no `/Popup` (ADR 0012; `docs/interop-profile.md`, "Replies"). Only their text and author change.

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

Each feature below defines behavior. The "settled details" were decided during the v1 build; change them only with the user's agreement.

### 6.1 Viewer

- **Opening:** File > Open dialog, drag-and-drop onto the window, `.pdf` file association, and a path passed on the command line. Opening a file that is already open switches to its tab. Lectrix is single-instance: a second launch hands its files to the running window.
- **Tabs:** one tab per document, reorderable, with a dirty marker (a dot; a spinning ring while saving) and a close prompt for unsaved changes.
- **Scrolling:** continuous vertical scroll, virtualized; only pages near the viewport are rendered.
- **Zoom:** fit width, fit page, preset percentages, Ctrl+wheel and pinch, centered on the cursor. Zooming glides to the new zoom in 140 ms: the + and − buttons and Ctrl+= / Ctrl+- (to the next preset, around the center of the view), the zoom menu, fit width, fit page (ending at the top of the current page) and mouse wheel notches (around the cursor). A step pressed during a glide goes on from where it was heading. A touchpad pinch follows the fingers instead, and re-fitting as the window or a pane changes size is immediate. Settings can turn gliding off, and reduced motion always does.
- **Navigation:** page box accepts a physical page number or a page label (typing `iv` jumps to the page labeled iv); a thumbnails panel; back/forward history for jumps (Alt+Left/Right).
- **Text:** selection and copy across lines and pages; search with match highlighting and next/previous.
- **View rotation:** rotating the view does not modify the document. A separate "Rotate pages" command does modify it (sets `/Rotate`) and is undoable.
- **Pages panel toolbar:** under the panel's title, rotate the current page counter-clockwise or clockwise (in the document, as "Rotate pages" does; the page's context menu in the viewer has no rotation, the thumbnail's does), make thumbnails smaller or larger (64 to 456 px wide, never wider than the panel), fit them to the panel's width (a toggle; they follow the width as the pane is resized, until Smaller or Larger is pressed), and switch between one column and a grid of as many columns as fit (one button whose icon shows the layout in use; arrow keys move across and down the grid). A size from these buttons glides there in 140 ms, as zooming does (off with smooth zooming, or reduced motion); resizing the pane follows it at once. The size, the fit and the layout are remembered in app data with the panes.
- **Recent files** list and remembered per-file view position (page and zoom), stored in app data, not in the PDF.

Settled details:

- Fit width and fit page use the current page when applied, and re-fit only when the window size or the view rotation changes, not while scrolling past pages of other sizes.
- Search is case-insensitive, starts at the current page and wraps. Search jumps are not recorded in back/forward history; page-box and thumbnail jumps are.
- Copying respects the document's copy permission. Rotating pages respects the modify or assemble permission, and the signed-document warning.

### 6.2 Bookmarks (outline)

- A panel (section 8) with the outline as a tree. Expanded/collapsed state is read from and written to the outline.
- **Add bookmark** (Ctrl+B) at the current page and scroll position; if text is selected, use it as the title.
- **Edit:** rename inline (F2 or double-click), drag to reorder and nest, delete (with children, after confirmation), "Set destination to current view."
- New and retargeted bookmarks get explicit destinations: page reference, `/XYZ`, left and top of the current view, null zoom (keeps the reader's zoom).
- Existing bookmarks the user did not edit keep their original destination or action exactly, including named destinations and URI actions.
- Titles with non-ASCII characters are written as UTF-16BE with a byte-order mark.

Settled details:

- A click selects a bookmark and follows it; arrow keys only move the selection; Enter follows. The selection stays when the page is clicked, so repeated Ctrl+B builds a list in order.
- Ctrl+B puts the new bookmark right after the selected one, as its sibling; with nothing selected, at the top level, in page order. Without selected text it is titled "Page <label>" and opens for renaming. Selected text is used only if the document allows copying.
- A new or retargeted bookmark points at the top-left of what is visible on the page at the top of the view (in the page's own orientation), rounded to whole points.
- Deleting a bookmark without children needs no confirmation (it can be undone).
- Expanding or collapsing is not an edit: it neither dirties the document nor adds an undo step, and is written at the next save. It is not written for a signed document without other changes, or when permissions forbid outline changes.
- Bookmarks untouched by an edit keep their bytes exactly; neighbours whose links change are rewritten by MuPDF with the same destination and action values.
- Web links, other files and actions are described in a notice when clicked, never opened or run. A bookmark's own color shows as a dot, not as text color.
- The inspector opens from the context menu (Properties), then follows the selection until closed. It floats over the page, so it never changes a fit-width zoom.

### 6.3 Page labels

- Edited as a list of rules. Each rule has a start page, a style (none, 1 2 3, i ii iii, I II III, a b c, A B C), an optional prefix, and a start number (1 or higher).
- A rule at the first page always exists (default: decimal from 1), because the format requires it.
- **Live preview:** the page box, the page position beside it and the rule list update while typing; thumbnail captions update when the edit is applied (Enter or leaving the field), before saving.
- **Presets:** "Roman front matter, then arabic from this page" and "Remove all labels."
- Letter styles follow the PDF convention: a…z, then aa…zz, then aaa…zzz.
- Opening a file shows its existing labels as rules exactly as stored; saving writes the `/PageLabels` number tree in the document catalog.

Settled details:

- A new range starts as "1, 2, 3 from 1", like Acrobat's "Begin new section".
- The roman front matter preset replaces every rule, later ones included, in one undo step. It is disabled on page 1.
- Deleting a range adds its pages to the range before it. The first range can't be deleted.
- Clicking a range selects it and shows its first page. Without a selection, the editor shows the range of the current page; focusing a field pins it.
- An invalid value shows why next to the fields ("Another rule already starts at page 5") and is not applied; leaving the field puts the stored value back.
- Stored rules that start after the last page have no effect: they are not listed, a note says so, and the next real change removes them. A file without a rule at page 1 shows "1, 2, 3 from page 1" there.
- A document without labels gets no `/PageLabels` tree until something is changed.
- Changing labels needs the same permission as rotating pages; in a signed document, the first change shows the signed-document warning.

### 6.4 Stitching (combine files)

- A Combine view: add files (dialog or drag-and-drop), see all their pages as thumbnails in one grid, and reorder, remove or rotate pages before combining.
- **Bookmark option** (default first): nest each source's outline under a new top-level bookmark named after the source's title or file name; or keep outlines flat; or drop them.
- **Page label option:** keep each source's labels (default); one continuous decimal sequence; or no labels.
- Annotations travel with their pages and still pass the interop profile afterwards.
- Internal links and named destinations are remapped to the new pages; colliding destination names and form field names are renamed with a per-source prefix, and the user is told how many were renamed.
- The result is always written to a new file via Save As. Source files are never modified.
- Also expose "Insert pages from file" into an open document, using the same engine.

Settled details:

- The Combine view is a tab of its own. Closing it asks first if pages were arranged but not combined. The result opens in a new tab, and the Combine tab stays open.
- Combining reads files as saved on disk. If one is open with unsaved changes, Lectrix offers "Save and combine", "Use saved version" or "Cancel".
- Files that forbid copying content are not added, with a plain message. Encrypted files that allow it ask for their password; the combined file is not encrypted.
- Before combining signed files, or inserting pages from one, Lectrix warns that the signature will not be valid in the result ("Combine anyway" / "Insert anyway", or Cancel). Signature fields are copied as they are.
- The same file can be added twice. Renamed names get a prefix by source position ("src2_chap2"), or "inserted_" when inserting, plus a number if that is taken too.
- Top-level bookmarks follow the order of each file's first page in the result. "Each page keeps its label" numbers unlabeled files 1, 2, 3… from their first page; when the result would be 1, 2, 3… throughout, no `/PageLabels` is written.
- Insert pages: the default position is after the current page. The file's bookmark goes among the top-level bookmarks in page order, and the view goes to the first inserted page.
- Not carried over: the structure tree, article threads, document-level JavaScript, open actions, viewer preferences and XMP metadata. The combined file is a plain full save.

### 6.5 Annotations (user interface)

- **Toolbar tools:** Select, Highlight, Underline, Strikeout, Squiggly, Note, Pen, Text box.
- **Text markup:** with a markup tool active, selecting text creates the annotation on mouse-up. Alt-drag creates an area highlight (one rectangular quad) for scanned pages without text. New text markup (from a tool, the quick tools or Alt-drag) is not selected, unless Settings' "Add a comment after marking text" is on: then it is selected and the inspector opens with the cursor in its note.
- **Quick tools:** with the Select tool, selected text gets a floating bar above the point where the pointer was released (below it when there is no room above; released on the last selected line, the bar keeps clear of that line), with the tools chosen in Settings: Highlight, Underline, Strikeout, Squiggly, Highlight with note, Copy, Add bookmark.
- **Annotation bar:** clicking an annotation without a comment selects it and shows a floating bar above it: color, type (text markup only: highlight, underline, strikeout, squiggly), note, delete. Clicking an annotation with a comment (not a text box, whose text is on the page) shows its inspector instead, with the bar's controls in its header. Double-clicking puts the cursor in the inspector's note; for an annotation without a comment, while the annotation list is on screen, it puts the cursor at the end of the annotation's comment in the list instead (the inspector, if the list's filters hide it).
- **Style:** Acrobat's 18 preset colors (three rows of six: bright colors, their light tints, white to black; arrow keys move among them) plus a custom color and opacity (a slider, 10% to 100%), in one panel that a button showing the color in use opens, as Acrobat's picker does; the same in the toolbar, the annotation bar and the inspector. The custom color's button opens Lectrix's own picker in the panel (open from the start when the color in use is not a preset): a saturation and brightness square, a hue bar, the eyedropper, and a field that takes a hex value only (six or three digits, no RGB or HSL). A color applies when picked (from the picker: when the pointer is let go, on Enter or leaving the field, or half a second after the arrow keys stop) and the panel stays open for the opacity, which applies when the slider is let go, stroke width for the pen; the last-used style per tool is remembered, including a color or opacity given to an existing annotation, which new ones of its type then get (Settings can turn this off: new annotations then start from the defaults each time Lectrix starts). Until changed, each tool has Acrobat's default: highlights yellow (`#FFD100`) at 40% opacity, notes yellow, underline, strikeout, squiggly, pen and text-box text red (`#E52237`); the yellow and red presets are these.
- **Inspector panel** (the comment panel) for the selected annotation: a header with the bar's controls (color, stroke width or font size, type, Properties where the bar has it, and delete at the right end), then note text, replies and repair. It sits beside the annotation (right, else left, else below), scrolls with the page, and a tail with curved sides points at the annotation's centre: from the side facing it, opposite that centre, or out of a corner when the centre is off that corner. It drags from anywhere but its controls, within the visible part of the view, and resizes from every edge and corner. Where it was put and the size it was given are kept for each annotation while the app runs. Other annotations' inspectors take the last size given as a limit: its width, and no more height than their content needs.
- **Properties dialog** for the selected annotation, from Properties in the page's context menu (or the bar of an annotation Lectrix can't change): the author, changed with Save (one undo step), and the creation and modification dates.
- **Annotation list** in the right pane by default (section 8): click to jump, edit note text, delete, "needs repair" badges. Its header shows how many annotations the document has ("5 of 12 annotations" while some are hidden) and three buttons. **Search** opens a field (sliding open, as the filters do) that filters the rows as you type, by their comments and replies, or, with the switch in the field, by the text they mark (text markup only; each page's text is read the first time). Esc empties the field, then closes it. **Sort** orders the rows by page (the default), author, date created or date modified, A–Z (first page, A, oldest first; the default) or Z–A, the order kept when the sort changes; rows without an author or a date go last either way. They show under headings of the page, the author or the day. **Filter** opens a panel below the header with the types present as their icons, the colours present as swatches and the authors as pills, each under its subheading (Type, Colour, Author); picking several in one row shows any of them, and the rows combine. A dot on the button marks picked filters; Clear filters empties them. Each row shows the type as an icon in the annotation's colour, moved toward the text colour only as far as 3:1 contrast on the pane needs (its name as tooltip and accessible name), then the author and a short date, then the whole comment with its line breaks. Each page's group starts with a header under a dividing line. A right-click (or Shift+F10) offers Reply, Copy comment and Delete annotation; on a reply in a thread, Edit reply, Copy comment and Delete reply.
- Annotations can be moved, resized (ink, text box, notes) and deleted; every change is undoable.
- Author name is set in Settings, defaulting to the Windows user name.

Settled details:

- The quick tools' markup uses each tool's last-used style; "Highlight with note" makes a highlight and opens the inspector on its note; "Add bookmark" does what Ctrl+B does. Tools the document forbids are shown disabled; with none allowed or none chosen, no bar appears. The default set is Highlight, Underline, Strikeout, Highlight with note and Copy.
- Each selected annotation shows either its inspector or its bar: the inspector if it had a comment when selected (it stays while the comment is edited, even emptied), or when opened from the bar, a double-click or the annotation list; otherwise the bar. Selecting another annotation decides again for that one. The inspector closes with Esc (from a control other than the note), and the bar takes its place until the annotation is clicked again, or when nothing is selected.
- Changing an annotation's type keeps its quads, color, opacity, note and author (one undo step, "Change highlight to underline"). The bar of a text box edits its text in place, as a double-click does; an annotation Lectrix can't change gets a Properties button instead.
- Notes are placed with one click; the inspector opens focused on the note field. Text boxes are typed in place; Ctrl+Enter or a click outside finishes, Esc discards. An empty new box is not created; emptying an existing one deletes it (one undo step).
- The inspector's color for a text box is its text color (rule 6 in section 5.1).
- Deleting an annotation that has replies asks first, then deletes the replies and the popup with it.
- A text box from another app that has a callout line can be edited but not moved or resized.
- The list shows markup, notes, drawings, text boxes, shapes, stamps and attachments; popups, links and form widgets are not listed.
- Reply opens a field under the thread; Ctrl+Enter or leaving it sends, Esc drops it, and an empty reply is not written. Edit reply edits in place the same way; emptying a reply deletes it. Replies are written only to annotations in the list's rows, not to replies, as Acrobat's list does; each is one undo step ("Add reply", "Edit reply", "Delete reply").
- The list's date is the last change, as Acrobat shortens it: the time for today, day and month this year, with the year before that; the full date and time are in its tooltip and in the Properties dialog.
- A selected row edits its comment in place in a field as tall as the text, at the same text size; it has no Delete button (the menu and the Delete key delete).
- Picking an annotation in the list scrolls the page to it with a 140 ms glide (Settings: "Smooth scrolling to annotations", on by default; reduced motion turns it off). A long way starts two screens from the annotation. Scrolling or clicking stops the glide.
- Resting the pointer on an annotation shows its comment in a tooltip, text only with its line breaks, at most 12 lines. It follows the pointer, below and right of it like a Windows tooltip, shifting left or above to stay wholly inside the page area. The delay is a Settings slider (0 to 2 s, 0.3 s by default). Text boxes show none, since their text is on the page.
- Comments' line breaks (CR, as Acrobat writes them, CR LF, LF, U+2028, U+2029) all show as breaks. A comment is written back only when its text was edited, so looking at one in a field changes nothing.
- Repair counts `/Rect` as too small only past the 1 pt margin. Unreadable QuadPoints are reported and left alone.
- Annotating a turned page places notes, drawings, text boxes and area highlights upright as seen on screen (text boxes carry `/Rotate`, as Acrobat writes); text markup follows the text.

### 6.6 Settings, About and installers

- **Settings** (File menu, Ctrl+,): groups as tabs with icons down the left side (Up and Down move between them; the last one shown opens again while Lectrix runs), the chosen group's settings at the right, one per row: the name and a description, then the control, a switch for anything that is on or off and a dropdown for a choice. **General:** author name; whether Lectrix checks for updates when it starts (on by default). **Appearance:** the theme (System by default, Light or Dark); under Motion, smooth zooming (on by default, section 6.1) and smooth scrolling to annotations picked in the list (on by default, section 6.5). **Annotations:** whether new text markup opens its comment (off by default, section 6.5); whether new annotations take the last color and opacity given to their type (on by default, section 6.5); the comment tooltip's delay (a slider, 0 to 2 s, 0.3 s by default, section 6.5). **Toolbars:** the annotation toolbar's placement (floating over the pages by default, or docked in the bar above them, always shown); for the floating toolbar, its edge (bottom by default, or top) and whether it shows only while the pointer is within about 72 px of that edge, while it has keyboard focus, and for 1.5 s after a tool is picked (off by default); the quick tools for selected text, a switch each. The groups and rows are declared once in `src/lib/features/settings/schema.ts`, which the dialog renders; a new setting is a field in Rust's `Settings` plus a row there. Each change applies at once and is stored in app data (typed text once typing pauses or the field is left); there is no Save button, and the dialog closes with its close button or Esc.
- **About** (Help menu): the app icon, version, the AGPL notice, MuPDF's credit, where the license files are installed, and the source code address with a Copy button. The address is shown, never opened (the update check is Lectrix's only network access).
- **Updates** (ADR 0011): after the startup files open, Lectrix asks GitHub for the latest release in the background. A newer one brings a floating notice: Update, Not now (asked again at the next start) or Don't ask again (that version is never offered again; a later one is). Update downloads with a progress bar and Stop, and the download must pass the updater signature check. macOS and Linux then install it and offer Restart now or Later. Windows cannot replace the running app, so it offers Restart now (after the usual unsaved-changes prompts, the installer runs and opens Lectrix again) or Later (the installer runs when Lectrix closes). Failures to check are logged, never shown.
- **Releases** (ADR 0011, `docs/releasing.md`): a new version in `package.json` on `main` publishes a GitHub release once CI passes: Windows (NSIS, MSI), Linux (AppImage, deb, and rpm for final versions) and macOS (Apple Silicon and Intel), with build provenance attestations and the updater's signed `latest.json`. Versions follow semver; pre-releases end in a number (`1.0.0-beta.1`) because the MSI version is derived from it, and installers are built with `npm run bundle`, which sets that MSI version.
- **Installers** (NSIS and MSI, ADR 0007): a "PDF files" page after the folder page, "Open PDF files with Lectrix", checked by default. It registers Lectrix for PDFs (Open with, Default apps), with the Lectrix PDF icon (`lectrix-pdf.ico`, installed next to the app) for PDFs it opens; Windows asks which app to use at the next PDF, and the installer never takes over the default itself. For silent installs, `/NOPDF` (NSIS) or `LECTRIX_ASSOCIATE_PDF=0` (MSI) leaves the registration out. Both install `LICENSE.txt` and the license notices.

## 7. Document model, undo/redo and saving

The Rust session is the single source of truth: the frontend holds only a view of it, and the undo history lives in Rust.

**Operations.** Every change is a variant of one `Operation` enum in `ops.rs` (for example `AddAnnotation`, `UpdateAnnotation`, `DeleteAnnotation`, `SetOutline`, `SetPageLabels`, `RotatePages`, `InsertPages`, `RepairAnnotations`). Applying an operation increments the document revision and returns what changed.

**Undo/redo.** Uses MuPDF's built-in journalling, wrapped in `ffi/journal.rs`; each `Operation` is one journal step with a human-readable name ("Add highlight", "Rename bookmark"). The Edit menu shows the name of the step being undone or redone. Saving reopens the file, so undo history starts again after each save (ADR 0003); after a crash restore it starts at the restored state (MuPDF cannot load a saved journal, ADR 0001).

**Dirty state.** A document is dirty when its revision differs from the last saved revision. Show a dot in the tab (a spinning ring while saving; there is no status bar), and prompt on close and on app exit.

**Atomic save.**

1. Write the new file to a temporary file in the same folder as the target.
2. Flush it to disk.
3. Replace the original in one rename on the same volume.
4. On failure, delete the temp file and leave the original untouched.

Documents are opened through a file stream that allows that rename (ADR 0003). If the target is locked by another program (Acrobat locks files it has open), show a clear message naming the likely cause and offer Save As.

**Crash recovery.** Every 2 minutes, each dirty document whose content changed since its last copy gets a recovery copy in the app's local data folder (a snapshot with the unsaved changes as an incremental update; damaged files are copied in full). Saving, reloading or closing a document deletes its copy, and so does quitting normally. On the next launch, before opening startup files, Lectrix offers to restore copies a crash left behind: Restore, Discard (asks once more) or Not now (asked again next time; Escape means Not now). A restored document opens in a tab for its own file, marked unsaved, and saves incrementally onto it. Encrypted files ask for their password again.

**External changes.** Watch open files. If a file changes on disk and the document has no unsaved edits, offer to reload; if it has unsaved edits, warn and offer Save As.

## 8. UI and UX guidelines

The app should feel like a native Windows 11 app: calm, fast, and keyboard-friendly, with the document always the visual focus.

**Layout.**

- **Title bar:** custom (Tauri decorations off, explicit drag region), holding the app's icon and name at the left, the menus, the document tabs and the standard window buttons, measured as VS Code's title bar (35 px tall, 16 px icon in a 35 px box, 12 px name, 13 px menus).
- **Side panes** hold four panels: Pages (thumbnails), Bookmarks, Page labels and Annotations (the annotation list). The left pane, open by default, starts with the first three; the right pane, closed by default, with Annotations. Each pane's top row, with a line under it, has its panels' tabs (icons with tooltips and accessible names) and, at its outer end, the button that hides it. A closed pane's button stays in the same place, at that end of the view bar's row, as in Obsidian. Panels move freely: a tab is dragged along its row, into the other pane's row, or onto a closed pane's button; Ctrl+Shift+Left/Right and the tab's context menu do the same from the keyboard, crossing between the panes at their inner ends. A panel moved into the other pane is shown there; a pane left without panels closes, and its button goes until a panel is dragged to the drop place shown there.
- **View bar**, one bar at the top of the document as in Acrobat, in the side panes' colour and continuous with their top rows (the line between a pane and the page starts below them), its contents centered: previous page, next page, the page box with the page label or number and the physical position after it ("iv" then "(4 of 312)", or "4" then "of 312"; screen readers hear "iv (4 of 312)"), then zoom out, the zoom level (a menu with fit width, fit page and the presets) and zoom in, then the annotation tools when docked there (Settings). Previous and next page go to the top of that page and are not recorded in back/forward history. In a narrow window the bar wraps onto a second row.
- **Center:** the page canvas, below the view bar, with the annotation toolbar floating at its bottom or top, or docked in the view bar (Settings). The search bar and the bookmark inspector move below a floating toolbar that stays at the top. The active tool has an accent icon and an accent border on every side; so do the type buttons of a selected annotation.
- The Annotations tab carries the "needs repair" dot, and so does its pane's button while that pane is closed.
- **Inspector**: for a bookmark, floating over the right edge of the page (section 6.2); for an annotation, a panel beside it on the page that can be dragged and resized (section 6.5). Shown when opened for it.
- **Side panes** resize by dragging their inner edge, or from the keyboard on that edge (arrows, Home, End); a double-click resets the width. Each takes at most 40% of the window. Which panes are open, their widths, the panels each holds and the one each shows are remembered in app data. Opening and closing slides (140 ms); a fit-width or fit-page view re-fits as a pane moves and renders again once it stops.

**Visual style.**

- Colours from Neo, the Obsidian theme: its light theme and its Midnight dark theme, with the greys tinted to Lectrix blue's hue, solid throughout (no Mica; ADR 0014).
- Follow the system light/dark setting by default; Settings can force light or dark. The accent is Lectrix blue from the brand kit (ADR 0010): `#2F5DAA` in the light theme, `#6FA3EF` in the dark one.
- As in Neo: document tabs are pills, the active one raised; menus, popovers, dropdown lists and notices are flat, with a thin edge and no shadow; clickable controls show the hand cursor (the window buttons keep the arrow). Dropdowns are the app's own (`Dropdown.svelte`), never the native `<select>`, whose list the system draws in its own colours.
- Font stack: Google Sans, bundled with the app (ADR 0009), then Segoe UI Variable, Segoe UI and the system UI fonts for characters it lacks.
- Spacing on an 8 px grid (4 px for tight spots); corner radius 6–8 px; thin borders instead of heavy shadows.
- All colors, sizes and radii as CSS variables (design tokens) consumed by Tailwind; no hard-coded colors in components.

**Behavior.**

- Never block the UI thread. Long operations (combining, repair, saving a large file) show progress and can be canceled.
- Placeholder page shapes at the correct size while pages render, so the scroll position never jumps.
- Errors in plain language with a suggested next step; never show raw error text or crash on bad input. Log details to a rotating log file.
- Respect reduced-motion settings; keep animations under 150 ms.

**Accessibility.** Every control is reachable by keyboard with a visible focus ring (the page canvas, which takes focus to scroll from the keyboard, has none: a ring around the document is noise), has an accessible name, and meets WCAG AA contrast (checked by `src/lib/contrast.test.ts` in both themes). Shift+F10 and the Menu key open the focused control's context menu. App-wide shortcuts such as Ctrl+S work while typing in panel fields.

**Default shortcuts** (not rebindable in v1). With a keyboard layout that types non-Latin letters (Russian, Greek...), letter shortcuts follow the key's place on a US keyboard, so Ctrl+Я is Ctrl+Z; Latin layouts such as AZERTY or Dvorak use the letter typed:

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

The suites keep the names they got during the build: `phase0` (labels, outline, merge and highlight from `pdf-cli`), `phase2` (bookmarks), `phase3` (labels), `phase4` and `phase4-local` (combining), `phase5` and `phase5-local` (annotations and repair). SumatraPDF and similar apps are built on MuPDF and do not count as an independent check.

**Manual release checklist** (`docs/manual-checklist.md`, performed by the user before every release; the agent prepares the files and keeps the checklist current): open the interop sample files in Acrobat Reader, Edge, Firefox and Foxit; for every annotation type confirm it is visible, has the right color and opacity, appears in the comments list, prints, and can be edited and deleted. Then the installers, a real crash, and a Narrator walk.

**Frontend and end-to-end:** Vitest for stores and component logic; `tauri-driver` with WebdriverIO for the critical flows (open, highlight, add bookmark, set labels, combine, save, reopen, crash recovery, recent files, and a keyboard-only accessibility walk).

**CI** (GitHub Actions, `windows-latest`): `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test`, `svelte-check`, Vitest, the interop harness, the end-to-end tests, the license check, an installer build uploaded as an artifact, and a silent install/uninstall check of both installers. macOS and Linux jobs build the app and its tests and run the tests without failing the job. After CI passes on `main`, the Release workflow publishes a version that has no release yet (ADR 0011).

## 10. Working rules for the agent

Follow these rules in every session; when a rule and a convenience conflict, the rule wins.

**Process.**

- Start each session by reading this document and `docs/status.md`.
- Work on one change at a time. For a new feature or a larger change, propose a plan and wait for the user's go-ahead before building.
- When a change is done and CI is green, report to the user: what was built, the tests, any measurements that changed, behavior choices made without explicit guidance, and decisions needed. Then stop for review.
- Keep the docs current: `docs/status.md` (add gaps you find, remove gaps you close), `docs/versions.md`, `docs/interop-profile.md`, `docs/manual-checklist.md`, and this document when the product's behavior changes.
- Make small commits using Conventional Commits (`feat:`, `fix:`, `test:`…). Every commit builds and passes tests.
- When you deviate from this document, write an ADR in `docs/decisions/` first, explaining what, why, and the alternatives considered.

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
