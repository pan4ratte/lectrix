# Third-party notices

Folio is licensed under AGPL-3.0-or-later (see `LICENSE`). It includes or links the
following third-party components. Every one has an AGPL-compatible license.

| Component | License | Use |
| --- | --- | --- |
| MuPDF 1.27.2 (Artifex Software) and its bundled third-party libraries (freetype, harfbuzz, jbig2dec, lcms2mt, libjpeg, openjpeg, zlib, brotli, gumbo, extract) | AGPL-3.0 (MuPDF); FTL/MIT/BSD-style/zlib (third-party) | PDF engine, statically linked via `mupdf-sys` |
| MuPDF public headers (`third_party/mupdf-include/`) | AGPL-3.0 | compiling the FFI shim |
| `mupdf`, `mupdf-sys` crates (messense/mupdf-rs) | AGPL-3.0 | Rust bindings; `mupdf` vendored with patches in `third_party/mupdf-rs/` |
| Tauri, tauri-build, tauri-plugin-dialog | MIT OR Apache-2.0 | app shell |
| thiserror | MIT OR Apache-2.0 | error types |
| cc | MIT OR Apache-2.0 | build-time C compilation |
| font-kit | MIT OR Apache-2.0 | system font lookup (via `mupdf`) |
| Svelte, SvelteKit, Vite | MIT | frontend |
| Tailwind CSS | MIT | styling |
| Bits UI | MIT | headless UI primitives |
| Lucide icons (`@lucide/svelte`) | ISC | icons |

Transitive Rust and npm dependencies are covered by their own license files. Before
release, generate the full list with `cargo about` / `license-checker` (Phase 6).
