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
  `third_party/mupdf-rs/FOLIO_PATCHES.md`:
  - a new `src/raw.rs` that adds `as_raw_ptr` accessors (borrowed, no ownership
    transfer) to the types above;
  - a standalone manifest with features trimmed to `base14-fonts`, `system-fonts` and
    `brotli`. JavaScript, Tesseract OCR, HTML/EPUB/XPS/CBZ/SVG input and DOCX output
    are out of v1 scope and only add build time and binary size.
- **`mupdf-sys` comes from upstream git at the same commit.** We do not vendor it,
  because it carries the full MuPDF and third-party C source tree.
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

| Need | Crate API | Folio |
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
| Merge with one graft map per source | `insert_pdf` grafts page by page with no shared map, so shared resources get duplicated | `ffi/graft.rs` + shim around `pdf_graft_mapped_page` |
| Header/library version check | none | shim `folio_mupdf_headers_match_library` (test) |
| Open from a share-delete OS handle (ADR 0003) | `PdfDocument::open` only takes a path | `ffi/stream.rs` + shim `folio_pdf_open_os_handle`; crate patch 3 (`from_raw_owned`) |
| Was the file repaired on open | none | `ffi::was_repaired` (shim around `pdf_was_repaired`) |
| Font lookup hook | `set_font_loader` | crate (`fonts.rs`: base-14 names go to MuPDF's built-in fonts) |
| Render into a pixmap with an origin (tiles) | `Pixmap::new`, `Device::from_pixmap_with_clip`, `DisplayList::run` | crate |
| Text geometry, search | `DisplayList::to_text_page`, `TextPage::search_cb` | crate |
| Signature detection | `pdf_count_signatures` counts unsigned fields too | own walk of `/AcroForm /Fields` in `docinfo.rs` |
| Save incremental / full | `PdfWriteOptions`, `save_with_options` | crate |
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
