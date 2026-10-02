# Folio patches to the vendored `mupdf` crate

Base: messense/mupdf-rs @ `537d50556ee8e4abf2435f81357dfef3c145d883` (crate version 0.8.0,
MuPDF 1.27.2). Rationale: `docs/decisions/0001-mupdf-binding-coverage.md`.

Only the `mupdf` crate is vendored (Rust source only). `mupdf-sys` and MuPDF's C sources
are fetched from upstream at the same commit.

| # | Files | Change |
|---|-------|--------|
| 1 | `src/raw.rs` (new), `src/lib.rs` (one `pub mod raw;` line) | Public raw-pointer accessors (`as_raw_ptr`) on `Context`, `Document`, `Page`, `PdfDocument`, `PdfPage`, `PdfAnnotation`, `PdfObject`, so Folio's `pdf-core/src/ffi/` can call MuPDF APIs the wrapper lacks. |
| 2 | `Cargo.toml` | Standalone manifest; features trimmed (no JS, OCR, HTML/EPUB/XPS/CBZ/SVG, DOCX output). |

To rebase onto a newer upstream: copy upstream `src/`, `LICENSE`, `README.md`, re-apply
the rows above, update both commit hashes, and bump `third_party/mupdf-include/` to the
matching MuPDF headers.
