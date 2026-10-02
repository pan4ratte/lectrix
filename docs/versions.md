# Pinned versions

Recorded at project start (2026-10-02). Versions are pinned exactly in `Cargo.toml`
(`=x.y.z`) and `package.json` (`save-exact`). Bump them deliberately and update this file.

## Toolchain

| Tool | Version | Notes |
| --- | --- | --- |
| Rust (stable, MSVC) | 1.99.0 | `rustup`, `x86_64-pc-windows-msvc` |
| MSVC Build Tools | 2022 17.14 (MSVC 14.44, Windows SDK 10.0.26100) | |
| LLVM / libclang | 23.1.2 | bindgen for `mupdf-sys`; `winget install LLVM.LLVM` |
| Node.js | 24.13.0 | npm 11.6.2 |
| qpdf | 12.4.2 | structural check (section 9); `winget install QPDF.QPDF` |
| Python | 3.14 | interop harness: pypdfium2 5.13.0 (PDFium 153.0.7999.0) |

## PDF engine

| Component | Version | Source |
| --- | --- | --- |
| MuPDF | 1.27.2 | built from source by `mupdf-sys` |
| `mupdf-sys` | 0.8.0 @ `537d50556ee8e4abf2435f81357dfef3c145d883` | git, upstream messense/mupdf-rs |
| `mupdf` | 0.8.0 @ same commit, Folio-patched | vendored in `third_party/mupdf-rs` (ADR 0001) |

## Rust crates (direct)

| Crate | Version |
| --- | --- |
| tauri | 2.12.1 |
| tauri-build | 2.7.1 |
| tauri-plugin-dialog | 2.8.1 |
| thiserror | 2.0.21 |
| cc (build) | 1.5.1 |

Tauri 3 is in alpha (3.0.0-alpha.4). Section 2 fixes Tauri 2, so we use the latest stable 2.x.

## Frontend (npm)

| Package | Version |
| --- | --- |
| svelte | 5.57.1 |
| @sveltejs/kit | 3.0.0 |
| @sveltejs/adapter-static | 4.0.0 |
| @sveltejs/vite-plugin-svelte | 7.3.1 |
| vite | 8.3.2 |
| tailwindcss, @tailwindcss/vite | 4.3.3 |
| bits-ui | 2.19.4 |
| @lucide/svelte | 1.50.0 |
| @tauri-apps/api, @tauri-apps/cli | 2.12.1 |
| @tauri-apps/plugin-dialog | 2.8.1 |
| typescript | 6.0.3 (SvelteKit 3 and svelte-check require ^6; TypeScript 7 is not supported yet) |
| svelte-check | 4.7.6 |
| vitest | 5.0.3 |

`npm audit` reports 5 low-severity advisories, all in a `cookie` copy nested under
bits-ui's own SvelteKit dependency (server-side cookie parsing). Folio ships a static
SPA with no server, so the code is never reached. We will re-check when bits-ui updates.
