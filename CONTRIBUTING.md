# Contributing to Lectrix

Thanks for taking a look. Issues and pull requests are welcome at <https://github.com/pan4ratte/lectrix>.

A bug report is most useful with the PDF that shows the problem, if you can share it, and with the name of the program that made the file. For a new feature or a larger change, please open an issue before writing code: some things are deliberately not built yet (filling forms, digital signatures, OCR, editing a page's text or images, redaction), and it is better to agree on the shape of one first.

[AGENTS.md](AGENTS.md) is the specification: how the code is laid out, the rules it is written by, and each feature in short. What every feature does in detail is in [docs/spec/](docs/spec/README.md), one file per feature. Both are addressed to coding agents, and read just as well for people. This page is the short way in.

## What you need

| Tool | Version | For |
| --- | --- | --- |
| Rust, stable | 1.99.0 | the backend and the PDF engine |
| A C compiler, and libclang | LLVM 18 or later | MuPDF is built from source, and its bindings are generated |
| NASM | 2.13 or later | libjpeg-turbo's SIMD code, which MuPDF decodes JPEG with (not needed on Apple silicon) |
| Node.js | 24 | the frontend |
| qpdf | 12 | the tests' structural check of every file they write |
| Python | 3.14 | the cross-renderer tests and the licence check |

The exact versions in use are in [docs/versions.md](docs/versions.md).

* **Windows:** the MSVC Build Tools 2022 with the Windows SDK, then `winget install LLVM.LLVM`, `winget install QPDF.QPDF` and `winget install NASM.NASM`, and add NASM's folder (`%LOCALAPPDATA%\bin\NASM`) to your PATH. `.cargo/config.toml` expects libclang in `C:\Program Files\LLVM\bin`; set `LIBCLANG_PATH` if yours is elsewhere.
* **macOS:** Xcode's command line tools, which bring libclang. `brew install qpdf`, and on an Intel Mac `brew install nasm`.
* **Linux** (Ubuntu 24.04 names): `libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev libayatana-appindicator3-dev libclang-18-dev qpdf nasm`.

Without NASM, Lectrix still builds, but its JPEG decoder has no SIMD code and is no faster
than MuPDF's own, and the
test `libjpeg_turbo_has_its_simd_code` fails. Installed afterwards, NASM is used once the
decoder is built again: `cargo clean -p mozjpeg-sys` (ADR 0024).

## Running it

```
npm ci
npx tauri dev
```

The first build compiles MuPDF, which takes several minutes; later ones are quick. A development build has a **Debug** menu after Help that shows every notification, notice and prompt on demand.

To build the installers for your system, run `npm run bundle`.

Engine bugs are easiest to pin down without the interface. `pdf-cli` runs every operation of the engine from the command line:

```
cargo run -p pdf-cli -- labels set in.pdf out.pdf --rule 0:roman-lower --rule 12:decimal
```

## Tests

CI runs all of these on every push, and a pull request needs them green:

```
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace     # also writes the TypeScript types in src/lib/ipc/generated
npm run check              # svelte-check
npx vitest run
python tests/licenses/notices.py --check
```

The **cross-renderer tests** draw every annotation Lectrix writes with three independent engines (MuPDF, PDFium and pdf.js) and fail if one of them shows nothing, or something different:

```
npm ci --prefix tests/interop
python -m pip install -r tests/interop/requirements.txt
cargo build -p pdf-cli
python tests/interop/run.py phase5     # and phase0, phase2, phase3, phase4, pages
```

The **end-to-end tests** drive the release app on Windows, so they need `npm run bundle` first:

```
cargo install tauri-driver --version 2.1.0 --locked
./tests/e2e/fetch-edgedriver.ps1
npm ci --prefix tests/e2e
npm test --prefix tests/e2e
npm run test:ci --prefix tests/e2e     # the same, with the app shown as CI's runners show it
```

Run `test:ci` before you push a change to these tests or to what they cover. CI's Windows runners ask for reduced motion and have a small screen (the window's page is 1028 px wide there), and a test can pass on your screen and fail on theirs. To run some test files only, name them after `--`: `npm run test:ci --prefix tests/e2e -- specs/motion.test.mjs`. The tests leave a Lectrix you have open alone, and need `pdf-cli` too (`cargo build -p pdf-cli`).

A test that fails only some of the time shows in CI one run at a time. The **soak run** finds them together: it runs the end-to-end tests and the Rust tests many times and lists every test that failed in any pass. Start it in GitHub (Actions > Soak > Run workflow), or run it yourself:

```
node tests/soak/run.mjs --passes 5 --like-ci             # both suites
node tests/soak/run.mjs --passes 10 --suite rust
```

Tests write to `target/test-output/` and nowhere else. Never weaken a test or a threshold to get a green run: say what the conflict is in the pull request instead.

## Layout

| Path | What is there |
| --- | --- |
| `crates/pdf-core/` | All PDF logic, as a plain Rust library with no Tauri dependency: sessions, rendering, text, annotations, bookmarks, page labels, combining, repair, undo, saving |
| `crates/pdf-cli/` | The command-line tool over `pdf-core` |
| `src-tauri/` | The app: commands the interface calls, the page-image protocol, windows, menus, dialogs, the updater, installers |
| `src/` | The interface, in Svelte 5 and Tailwind. Feature code is in `src/lib/features/<feature>/` |
| `third_party/` | The vendored `mupdf` crate with Lectrix's patches, and MuPDF's headers |
| `tests/` | The cross-renderer, end-to-end, installer, licence and performance tests |
| `design/` | The sources of the icons and the wordmark |
| `docs/` | Feature specifications (`docs/spec/`), decision records (`docs/decisions/`), the annotation write profile, measurements and known gaps (`docs/status.md`) |

## Rules worth knowing first

AGENTS.md has all of them. These are the ones a first pull request most often meets:

* **PDF bytes are read and written only in Rust.** The interface shows rendered pages and sends typed operations; it never parses a PDF. Every change to a document is a variant of `Operation` in `pdf-core/src/ops.rs`, and is one undo step.
* **What Lectrix writes must open everywhere.** Annotations follow the write profile in [docs/interop-profile.md](docs/interop-profile.md), and annotations the user did not touch keep their bytes. The profile is not relaxed to make something pass.
* **Never guess a MuPDF function.** Check the headers in `third_party/mupdf-include/`. A missing binding gets the smallest safe wrapper in `pdf-core/src/ffi/`, with a `// SAFETY:` comment on each `unsafe` block and a test.
* **Types that cross between Rust and TypeScript are generated** by `cargo test -p lectrix`, never written by hand. Commit what it writes.
* **Rust:** no `unwrap()` or `expect()` outside tests unless the invariant is written beside it, errors through `thiserror`, and no panic may reach the interface.
* **TypeScript and Svelte:** strict mode, no `any`, runes for state.
* **The look comes from design tokens.** No colour, size or radius is hard-coded in a component. Tooltips are named in `data-tip`, not `title`, and every control has an accessible name and a visible focus ring.
* **Anything specific to one system** goes in a `platform` module behind a trait.
* **A new dependency** must have a licence compatible with the AGPL (MIT, Apache-2.0, BSD, MPL-2.0), be pinned to an exact version, and be added to `THIRD_PARTY_NOTICES.md` and `docs/versions.md`. Please ask in an issue before adding a large one.

## Commits and pull requests

* Commits follow [Conventional Commits](https://www.conventionalcommits.org) (`feat:`, `fix:`, `test:`, `docs:`…), and each one builds and passes the tests.
* A change people will notice gets a line in [CHANGELOG.md](CHANGELOG.md) under `## Unreleased`, in the same commit. [docs/releasing.md](docs/releasing.md) says how it is worded: one plain sentence about what people can do or see now.
* A change to how the app behaves is also written into its feature's file in `docs/spec/`, so that the specification stays true.
* A decision that departs from AGENTS.md or a spec gets a short record in `docs/decisions/` saying what, why, and what else was considered.

How a release is made is in [docs/releasing.md](docs/releasing.md).

## Licence

Lectrix is licensed under the [GNU AGPL v3](LICENSE) or any later version. By contributing you agree that your contribution is licensed the same way. The app icon and the PDF file icon are not part of that: they are © Mark Ingrem, all rights reserved.
