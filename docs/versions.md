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
| `mupdf-sys` | 0.8.0 @ `537d50556ee8e4abf2435f81357dfef3c145d883` + Lectrix patch | git, Lectrix's fork pan4ratte/mupdf-rs, branch `folio-mupdf-1.27.2` @ `460b796a88f2fa4ba617b07fbb7f73e5308f1417` (upstream commit plus `folio_patches.rs`: JPEG 2000 decoded at the resolution drawn, ADR 0008), via `[patch]` in `Cargo.toml` |
| `mupdf` | 0.8.0 @ same commit, Lectrix-patched | vendored in `third_party/mupdf-rs` (ADR 0001); built without `system-fonts` (ADR 0005), so `font-kit` is no longer in the build |

## Rust crates (direct)

| Crate | Version |
| --- | --- |
| tauri | 2.12.1 |
| tauri-build | 2.7.1 |
| tauri-plugin-dialog | 2.8.1 |
| tauri-plugin-single-instance | 2.5.2 (forwards a second launch's files to the running window) |
| tauri-plugin-updater | 2.13.1 (updates from GitHub releases, ADR 0011; brings reqwest 0.13, rustls 0.23 with ring, rustls-platform-verifier, minisign-verify, zip) |
| windows-sys | 0.61.2 (Windows `platform` module; already in the tree through Tauri) |
| windows | 0.62.2 (`pdf-core` DirectWrite font index, ADR 0005; already in the tree through Tauri's webview2-com) |
| webview2-com | 0.39.1 (WebView2 memory target level while minimized; already in the tree through Tauri) |
| windows-core | 0.62.2 (COM interface casts for the above; already in the tree) |
| thiserror | 2.0.21 |
| cc (build) | 1.5.1 |
| uuid | 1.26.1 |
| png | 0.18.1 |
| serde | 1.0.229 |
| ts-rs | 12.0.1 |
| clap (pdf-cli only) | 4.6.7 |

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
| typescript | 6.0.3 (SvelteKit 3 and svelte-check require ^6; TypeScript 7 is not supported yet) |
| svelte-check | 4.7.6 |
| vitest | 5.0.3 |

The Open dialog runs on the Rust side (`tauri-plugin-dialog` crate), so the app needs no
dialog JavaScript package and the webview has no dialog permission.

SvelteKit 3 notes: configuration lives in `vite.config.ts` (`sveltekit({ adapter, ... })`;
`svelte.config.js` is no longer read), `tsconfig.json` extends `$app/tsconfig`, and
`$lib` is replaced by the subpath import `#lib/*` (package.json `imports`), used with
explicit file extensions.

## Interop harness (tests/interop, not shipped)

| Package | Version | License |
| --- | --- | --- |
| pdfjs-dist | 6.3.289 | Apache-2.0 |
| @napi-rs/canvas | 1.0.10 | MIT |
| pypdfium2 (PDFium 153.0.7999.0) | 5.13.0 | Apache-2.0 / BSD-3-Clause |
| numpy | 2.5.3 | BSD-3-Clause |
| Pillow | 12.2.0 | MIT-CMU |

`npm audit` reports 5 low-severity advisories, all in a `cookie` copy nested under
bits-ui's own SvelteKit dependency (server-side cookie parsing). Lectrix ships a static
SPA with no server, so the code is never reached. We will re-check when bits-ui updates.

## Installers

Downloaded by the Tauri CLI (2.12.1) on the first `npx tauri build` into
`%LOCALAPPDATA%\tauri`, not pinned by Lectrix:

| Tool | Version | Notes |
| --- | --- | --- |
| NSIS | 3.11 | with `nsis_tauri_utils` 0.5.3; Lectrix's template is a modified copy of the CLI's own (`src-tauri/windows/installer.nsi`, ADR 0007) |
| WiX Toolset | 3.14 | `src-tauri/windows/pdf-association.wxs` is added as a fragment (ADR 0007) |

When the Tauri CLI is upgraded, re-apply the `Lectrix:` changes to its new NSIS template.

Release builds (`.github/workflows/release.yml`, ADR 0011) run on `windows-latest`,
`ubuntu-24.04`, `macos-latest` (Apple Silicon) and `macos-15-intel`, with the same
toolchain and libraries as CI, and `actions/attest-build-provenance@v3` for the
attestations.

macOS and Linux CI jobs (build only): `macos-latest` with Xcode's libclang, and
`ubuntu-24.04` with `libwebkit2gtk-4.1-dev`, `libgtk-3-dev`, `librsvg2-dev`,
`libayatana-appindicator3-dev` and `libclang-18-dev`.

## End-to-end tests (tests/e2e, not shipped)

| Tool | Version | License |
| --- | --- | --- |
| webdriverio (standalone, with Node's built-in test runner) | 9.32.0 | MIT |
| tauri-driver | 2.1.0 (`cargo install tauri-driver --locked`) | Apache-2.0 / MIT |
| msedgedriver | matches the installed WebView2 runtime (154.0.4258.53 here); fetched by `tests/e2e/fetch-edgedriver.ps1` | Microsoft |

`npm audit` in `tests/e2e` reports advisories in `basic-ftp` (through webdriverio's
proxy-agent chain, used only to download browsers through a proxy). The suite talks to a
local driver and downloads nothing, so the code is never reached.

On Windows, tauri-driver passes `tauri:options.args` to WebView2 rather than to the app,
so the tests open files through the `LECTRIX_OPEN` environment variable (paths separated by
`;`), which the app treats like command-line arguments.

msedgedriver turns on remote debugging through `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS`.
Tauri always passes its own default arguments through the WebView2 API, and some runtimes
(153, on CI's Windows Server 2025) then ignore the variable, so no session could start.
Lectrix creates its main window in code and, when the variable is set, passes Tauri's
defaults and the variable's arguments merged (`platform/windows.rs`).
