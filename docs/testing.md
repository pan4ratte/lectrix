# Testing strategy and interop harness

Part of the Lectrix specification. [AGENTS.md](../AGENTS.md) holds the rules every change follows, with section 9 in short; this file is section 9 in full, and keeps its numbers, so a reference to it still leads here.

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

The suites keep the names they got during the build: `phase0` (labels, outline, merge and highlight from `pdf-cli`), `phase2` (bookmarks), `phase3` (labels), `phase4` and `phase4-local` (combining), `pages` (moving and deleting pages, named for what it checks), `phase5` and `phase5-local` (annotations and repair), and `compress` (the presets and options of Compress on a generated file of every kind of image, each engine's render against its own of the original). SumatraPDF and similar apps are built on MuPDF and do not count as an independent check.

**Manual release checklist** (`docs/manual-checklist.md`, performed by the user before every release; the agent prepares the files and keeps the checklist current): open the interop sample files in Acrobat Reader, Edge, Firefox and Foxit; for every annotation type confirm it is visible, has the right color and opacity, appears in the comments list, prints, and can be edited and deleted. Then the installers, a real crash, and a Narrator walk.

**Frontend and end-to-end:** Vitest for stores and component logic; `tauri-driver` with WebdriverIO for the critical flows (open, highlight, add bookmark, set labels, combine, save, reopen, crash recovery, recent files, tabs and windows, full screen, organizing pages, printing, and a keyboard-only accessibility walk). The end-to-end tests run beside a Lectrix the user has open and leave it alone: a test run (`LECTRIX_EPHEMERAL`) is an instance of its own, outside the single-instance hand-off, and the launcher ends only copies of the app under test (`tests/e2e/lib/app.mjs`). **Run them as CI shows the app before pushing** (`npm run test:ci --prefix tests/e2e`, one or more test files after `--`): CI's Windows runners ask for reduced motion and have a 1024 x 768 screen, where the window's page is 1028 px wide and the view bar wraps sooner, and a test written on a larger screen with animations on can fail there alone. The mode passes the browser its reduced-motion switch (`LECTRIX_BROWSER_ARGS`) and gives a window without a remembered place that size (`LECTRIX_WINDOW_SIZE`). `LECTRIX_AUDIENCE_ANY_SCREEN=1` lets a presentation's audience screen open on the only screen there is, over the presenter view, so the presenter view is tested on a machine with one screen. A test of motion checks the way there only where motion is allowed (`matchMedia('(prefers-reduced-motion: reduce)')`), and where things end up either way.

**Debug menu** (development builds, or a release build run with `LECTRIX_DEBUG=1`; `LECTRIX_DEBUG=0` hides it): after Help, it shows each notification, notice and prompt on demand, worded as the app words them: the update notice at every stage, the note after an update (start screen or notification), information and error notifications, the notification that a save removed a PDF/A claim, the file-changed banner, the banner of a PDF/A file opened for reading only (the open document can't be changed until Enable editing or Clear all) and the crash recovery offer, plus Clear all. Rehearsals change nothing in Rust or on disk: the update notice's buttons work on a stand-in that pretends to download and to restart. The rehearsed update brings what `CHANGELOG.md` lists under Unreleased, so the next release's notes can be read as users will see them. A new kind of notification gets an item there (`src/lib/features/debug/`).

**CI** (GitHub Actions): on `windows-latest`, five jobs. `checks`: `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test`, `svelte-check`, Vitest and the license check. `build`: the release app, its installer (uploaded as an artifact), `pdf-cli` and `tauri-driver`, once. On that build, side by side: `interop` (the interop harness), `installer` (a silent install/uninstall check of the installer, which also has it take over an install made by the 1.1.0 MSI) and `e2e` (the end-to-end tests). Every job, and every step of `checks`, runs whatever the others do, so one run reports every failure, and a job that failed is run again by itself (Re-run failed jobs), not with the rest. macOS and Linux jobs build the app and its tests and run the tests without failing the job. After CI passes on `main`, the Release workflow publishes a version that has no release yet (ADR 0011).

**Soak run** (`.github/workflows/soak.yml`, `tests/soak/run.mjs`): the end-to-end tests and the Rust tests ten times over, each pass to its end, then one list of every test that failed in any pass, with how often and why, on the run's page. It finds the tests that fail only some of the time, which a CI run shows one at a time, on the day each happens to fail. Started by hand (Actions > Soak), and every night when `main` changed that day; it gates nothing. Run it before a release, fix what it lists in one commit, and run it again. Locally: `node tests/soak/run.mjs --passes 3 --like-ci`.
