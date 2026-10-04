# 0001: MuPDF bindings: vendored `mupdf` crate, coverage, thread safety

- Status: accepted (Phase 0)
- Date: 2026-10-02

## Context

AGENTS.md section 2 says to use MuPDF through the `mupdf` Rust crate, and to call the C API
directly through `mupdf-sys` where the crate lacks coverage. Phase 0 asks for a list of
which MuPDF APIs the crate covers, which needed `ffi/` wrappers, and the thread-safety
findings.

Three problems came up with the latest release, `mupdf` 0.8.0 (MuPDF 1.27.2):

1. **0.8.0 does not build on windows-msvc with current LLVM.** bindgen with libclang 22+
   does not emit `max_align_t` (on MSVC a `typedef double`), and `src/device/native.rs`
   needs it. Upstream fixed this on `main` (messense/mupdf-rs#258), but no release
   includes the fix yet.
2. **No raw handles are public.** `Context`, `PdfDocument`, `PdfPage`, `PdfAnnotation`
   and `PdfObject` keep their pointers `pub(crate)`. That makes "call `mupdf-sys`
   directly where the crate lacks coverage" impossible: there is no `fz_context*` or
   `pdf_document*` to pass.
3. **Raw MuPDF calls are not safe to make from Rust.** MuPDF signals errors with
   `longjmp`. A throw that crosses Rust frames is undefined behaviour, and a throw with
   no `fz_try` frame aborts the process. `mupdf-sys` wraps the functions it uses in C
   `fz_try` shims, but it has no wrappers for the APIs we need (for example journalling).

## Decision

- **Vendor the `mupdf` crate (Rust source only)** at upstream commit
  `537d50556ee8e4abf2435f81357dfef3c145d883` into `third_party/mupdf-rs/`. This picks up
  the #258 fix and the per-thread context work (#263, #264). The patches are listed in
  `third_party/mupdf-rs/LECTRIX_PATCHES.md`:
  - a new `src/raw.rs` that adds `as_raw_ptr` accessors (borrowed, no ownership
    transfer) to the types above;
  - a standalone manifest with features trimmed to `base14-fonts`, `system-fonts` and
    `brotli`. JavaScript, Tesseract OCR, HTML/EPUB/XPS/CBZ/SVG input and DOCX output
    are out of v1 scope and only add build time and binary size.
- **`mupdf-sys` comes from upstream git at the same commit.** We do not vendor it,
  because it carries the full MuPDF and third-party C source tree. Since Phase 6 it comes
  from Lectrix's fork of `mupdf-rs` at that commit plus one build-time patch (JPEG 2000
  decoded at the resolution drawn), through `[patch]` in the workspace (ADR 0008).
- **Vendor MuPDF 1.27.2's public headers** (`third_party/mupdf-include/`, about 1 MB) so
  `crates/pdf-core/build.rs` can compile `src/ffi/shim.c` with the `cc` crate. Every
  shim function wraps its MuPDF calls in `fz_try`/`fz_catch` and returns an error code
  plus a copied message. A test (`ffi::tests::headers_match_linked_library`) creates a
  context with the header `FZ_VERSION`; MuPDF refuses if that differs from the linked
  library, so a header/library mismatch fails CI.
- **libclang**: bindgen needs it. On Windows we install LLVM (`winget install LLVM.LLVM`),
  and `.cargo/config.toml` points `LIBCLANG_PATH` at it unless the environment already
  sets it.

We will offer the accessor patch upstream. Once a release has both the #258 fix and
public raw access, we drop the vendored copy and depend on crates.io again.

## Coverage (MuPDF 1.27.2, crate @ 537d505)

| Need | Crate API | Lectrix |
| --- | --- | --- |
| Open, page count, page load, bounds | `Document`, `PdfDocument::open`, `load_pdf_page` | crate |
| Render | `Page::to_display_list`, `DisplayList::to_pixmap` | crate |
| Structured text, search | `TextPage`, `Page::to_text_page` | crate |
| Page labels (read one label) | `PdfDocument::page_label` | crate |
| Page labels (write) | `set_page_label_rule` (one rule per call, MuPDF semantics) | own number-tree writer in `labels.rs` on `PdfObject`, for exact round-trip control |
| Outline write | `set_outlines` rebuilds the whole tree; no `/Count`, no open state, no `/XYZ` null zoom | own writer in `outline/mod.rs`, and an in-place editor in `outline/edit.rs` that writes only changed keys (Phase 2; section 6.2 needs untouched items preserved) |
| Named destinations | none (`pdf_lookup_dest` is not wrapped and returns a borrowed object) | own `/Dests` dictionary and name-tree lookup in `outline/names.rs` |
| Annotations: create, quads, colour, opacity, author, contents, ink, popup, `update` (appearance synthesis) | `PdfPage::create_annotation`, `PdfAnnotation::*` | crate; QuadPoints and `/Rect` written by `annot/` through one quad writer |
| Journalling: begin/end/abandon operation | `PdfDocument::begin_operation` etc. | crate |
| Journalling: enable, undo, redo, state, step names, implicit operations | none | `ffi/journal.rs` plus the shim (implicit operations write expanded bookmark states at save without an undo step, Phase 2) |
| Copy pages between documents (combine, insert) | `insert_pdf` / `pdf_graft_mapped_page` copy contents, resources and boxes only: annotations (and links, widgets) are left out, and `/Group` too | own copier in `merge/copy.rs` on the crate's `PdfObject` API (Phase 4, below); the Phase 0 graft wrapper is gone |
| Write the stream of an object created in the current undo step, at a cost that does not grow with the step | `write_raw_stream_buffer` (MuPDF scans the step's records on every change) | `ffi::set_new_stream`: shim sets the journal aside for that one write (Phase 4, below) |
| Header/library version check | none | shim `lectrix_mupdf_headers_match_library` (test) |
| Open from a share-delete OS handle (ADR 0003) | `PdfDocument::open` only takes a path | `ffi/stream.rs` + shim `lectrix_pdf_open_os_handle`; crate patch 3 (`from_raw_owned`) |
| Was the file repaired on open | none | `ffi::was_repaired` (shim around `pdf_was_repaired`) |
| Font lookup hook | `set_font_loader` | crate (`fonts/`: base-14 names go to MuPDF's built-in fonts, other names to Lectrix's installed-font index; the `system-fonts` feature is off since Phase 2, ADR 0005) |
| Render into a pixmap with an origin (tiles) | `Pixmap::new`, `Device::from_pixmap_with_clip`, `DisplayList::run` | crate |
| Text geometry, search | `DisplayList::to_text_page`, `TextPage::search_cb` | crate |
| Signature detection | `pdf_count_signatures` counts unsigned fields too | own walk of `/AcroForm /Fields` in `docinfo.rs` |
| Save incremental / full | `PdfWriteOptions`, `save_with_options` | crate |
| Snapshot for crash recovery (file plus unsaved changes, without finalizing them) | none | `ffi::save_snapshot` (shim around `pdf_save_snapshot`), `ffi::has_unsaved_changes` (Phase 6, below) |
| Raw object access | `PdfObject` dict and array API, `catalog`, `trailer` | crate |

This table is updated as `ffi/` grows.

Other findings from Phase 0:

- `mupdf-sys`'s `mupdf_pdf_insert_page` wrapper rejects `-1` ("append"); pass the page
  count instead.
- `PdfDocument::open` keeps the file open without `FILE_SHARE_DELETE` on Windows, so an
  atomic replace of the open file fails with a sharing violation (verified). Phase 1 must
  open documents in a way that allows the rename (see `docs/progress.md`).
- With the `system-fonts` feature, the first non-embedded font lookup enumerates the
  Windows font collection through `font-kit` (about 1.6 s once per process). See
  `docs/progress.md` for measurements and options.

Phase 4 (combining files, inserting pages): `pdf_graft_mapped_page` copies only a page's
contents, resources, boxes, `/Rotate` and `/UserUnit`, so annotations, links and form
widgets would be lost, and grafting an annotation directly would copy the whole source
document (its `/P` leads to the page, whose `/Parent` leads to the page tree). Lectrix's
copier keeps its own map: picked pages are mapped to their new pages first, and the
catalog, page tree nodes and unpicked pages are marked never to be copied. It copies
through a queue rather than by recursion, and raw stream data (decrypted, still encoded)
as MuPDF's graft does. Two crate behaviours to know (also in the code): `PdfObject`'s
`try_clone`/`clone` deep-copies (`pdf_deep_copy_obj`), so an object must be read back from
its container to change it there; and MuPDF's null object is a null pointer, which the
crate's `array_iter`/`dict_iter` report as an error (`objects::array_items` and
`dict_entries` skip it). MuPDF 1.27's `pdf_create_document` also puts `/Info` inside the
catalog; combined files move it to the trailer.

Big undo steps: MuPDF records the first change to each object in an undo step by scanning
everything the step has already recorded, and `pdf_insert_page` runs a nested step per
page whose merge (`resolve_undo`) is quadratic too. Inserting a 2,881-page book in one
step took 197 s. Inserted pages therefore join the tree as one new `/Pages` node, written
with their `/Parent` already set, and the stream data of objects created in the step is
written with the journal set aside (`ffi::set_new_stream`); undo removes such objects
whole, with their streams, so nothing needs recording. The same insert now takes 0.2 s.

Phase 6 (crash recovery, AGENTS.md section 7): recovery copies are MuPDF snapshots
(`pdf_save_snapshot`): the document's file followed by its unsaved changes as an
incremental update, written without finalizing that update in memory. MuPDF records no
journal changes while saving (`save_in_progress`, `pdf-object.c`), and the empty "Save
document" step it opens is dropped, so the document, its undo and its redo history stay
exactly as they were (tested). Reopened, a snapshot is an ordinary file whose last update
holds the changes, so saving a restored document in place keeps the original bytes as its
prefix (section 5.4). Snapshots need an incremental write, which MuPDF refuses for
repaired files; those are copied with a full save, which also records nothing in the
journal.

MuPDF can save the undo history next to a snapshot (`pdf_save_journal`) and load it back
(`pdf_load_journal`), which would let a restored document undo the changes it came back
with. In 1.27.2 loading fails as soon as the journal records any change:
`pdf_deserialise_journal` links its entries into the history list (`new_entry`), but
`pdf_add_journal_fragment` only appends to the pending operation (`pending_tail`), which
is empty while loading, so the first fragment throws "Can't add a journal fragment absent
an operation". The journal's structs are private to `pdf-object.c`, so the shim cannot
work around it. A restored document's undo history therefore starts at the restore. This
could be reported upstream; if a later MuPDF fixes it, restoring the history is a small
change (save the journal beside the snapshot, load it in `Session::restore`).

## Thread safety

- The crate gives every thread its own `fz_context`, either cloned from one process-wide
  base context (shared store and locks) or an independent family via
  `init_thread_context`. This is the pattern MuPDF's multi-threading guide describes.
- `Document`, `PdfDocument`, `Page`, `PdfPage`, `PdfAnnotation` and `PdfObject` are
  **not `Send`**: they hold raw pointers with no `Send` impl. So a document must live on
  one thread, which matches the actor design (AGENTS.md section 3): one actor thread per
  open document owns it exclusively.
- `DisplayList` is **`Send + Sync`**, so display lists built on the actor thread can be
  rendered on a worker pool. Each worker uses its own cloned context from the same base
  family, which is required because objects can only be used with contexts of the
  family that created them.
- Our `ffi` wrappers take `&mut PdfDocument`, so they inherit its thread confinement, and
  they always use the calling thread's `Context::get()`.

## Alternatives considered

- **Downgrade LLVM** to a version that emits `max_align_t`: this fixes only problem 1,
  and it breaks again for every contributor with a newer LLVM.
- **Use `mupdf-sys` only, with no safe crate**: full control, but it means rewriting
  thousands of lines of wrapper code the crate already has.
- **Transmute the crate's structs to reach their pointers**: the struct layout is not
  `repr(C)`, so this would be undefined behaviour.
- **Fork on GitHub and use a git dependency**: equivalent to vendoring, but it adds an
  external repository to maintain. We may still switch to this when upstreaming.
