# Status

**v1 is complete (2026-10-04).** All seven build phases were finished, reviewed by the
user and passed CI. This file lists what v1 measures and what it does not do yet. Keep it
current when a gap closes or a new one is found.

The detailed phase reports (what each phase built, its measurements, the decisions asked
for and the user's answers) were kept in `docs/progress.md` until v1 was complete. They
remain in git history: `git show 565fdb1:docs/progress.md`. ADRs that cite
`docs/progress.md` refer to that file.

## What v1 contains

Everything in AGENTS.md sections 1 and 6: the viewer with tabs, bookmarks, page labels,
combining files and inserting pages, the seven annotation types with an annotation list,
and repair. Also: undo and redo for every edit, atomic and incremental saving, crash
recovery, recent files with each file's last view, Settings (author name; System, Light
or Dark appearance), an About dialog with the AGPL source address, and NSIS and MSI
installers that can register Lectrix for PDF files (ADR 0007).

Since v1: releases on GitHub for Windows, Linux and macOS, built by the Release
workflow with attested installers, and an in-app updater with a Settings switch
(ADR 0011). Replies to annotations, written in the annotation list (ADR 0012).

## Performance

Last measured 2026-10-04: release build, PNG page images, a mid-range laptop.

| Target (AGENTS.md section 2) | Measured | Met? |
| --- | --- | --- |
| Window visible < 1 s | 30 to 99 ms | Yes |
| First page visible < 1 s | 1,000-page file 73 to 80 ms; 2,881-page dictionary 137 ms; a 33-megapixel JPEG 2000 scan 376 to 382 ms | Yes |
| No blank page > 200 ms while scrolling | 1,000-page file: none at steady speed, worst 18 ms fast, 46 to 54 ms jumping | Yes, except JPEG 2000 scans (below) |
| `lectrix.exe` < 200 MB idle (ADR 0002) | 63 to 64 MB with the 1,000-page file, 101 MB with a JPEG 2000 book | Yes |
| WebView2 growth rule (ADR 0002) | Whole process tree 635 to 647 MB idle; the growth limits held when last checked | Yes |

Scripts: `tests/perf/measure.ps1` and `tests/perf/memory-over-time.ps1`.

## Known gaps

- **Scrolling JPEG 2000 scans.** Books scanned as JPEG 2000 (archive.org style) show blank
  pages while scrolling: up to about 0.4 s at speed, and up to 1 s for the first pages.
  These numbers were measured before the reduced-resolution patch (ADR 0008), which makes
  thumbnails cheaper; scrolling was not re-measured. Each page takes 0.2 to 0.4 s to
  decode, and MuPDF decodes one JPEG 2000 image at a time, on one thread, under a global
  lock (`fz_opj_lock`). Lifting that is left to upstream MuPDF.
- **Undo history restarts** after each save (ADR 0003) and after restoring a crash
  recovery copy: MuPDF 1.27.2 cannot load a saved journal (ADR 0001).
- **Recovery copies of large damaged files** are full copies and take 1.2 to 1.5 s for
  165 to 173 MB files. That document renders nothing new meanwhile. Copies are written at
  most every 2 minutes, and only after a change.
- **Combining** does not carry over the structure tree (the result is not tagged),
  article threads, document-level JavaScript, open actions, viewer preferences or XMP
  metadata. Signatures from signed sources are invalid in the result. Lectrix warns
  before combining or inserting from them.
- **Replies in Acrobat and Foxit** have not been checked by a person yet (ADR 0012): they
  are meant to show in the parent's thread there, which no engine the harness runs can
  confirm. The manual checklist covers it.
- **Accessibility.** Creating, moving and resizing annotations needs a pointer, and so
  does selecting text, which brings up the quick tools. A reply's own menu (Edit reply,
  Delete reply) opens only with a right-click; Reply itself is in the row's menu, which
  the keyboard reaches. Editing, deleting and repairing
  work from the keyboard; the bar of a selected annotation is reached with Tab from the
  page. Page text is not exposed to screen readers, and Windows high-contrast themes are
  not handled specifically.
- **Not covered by automated tests:** drag-and-drop from Explorer (an OLE drag can't be
  scripted), touch or stylus input, and touchpad pinch zoom.
- **Touchscreen pinch** does not zoom: only touchpad pinch and Ctrl+wheel do. The app
  blocks touchscreen pinch (`touch-action`) so it cannot scale the whole window.
- **Test corpus.** `tests/corpus/` is still empty. Geometric cases come from generated
  files (`pdf-core/src/testgen.rs`), and real-world coverage comes from the local corpus,
  which lacks CJK text, signed files, UserUnit pages and annotations made by other apps
  of these types: Squiggly, Circle and FileAttachment.
- Engine differences that no file can change (PDFium ignores `/UserUnit`, and sticky
  notes turn with rotated pages in PDFium and pdf.js) are listed in
  `docs/interop-profile.md`.

## Platform gaps (macOS and Linux)

CI builds the app and its tests on both, and runs the tests without failing the job.
Nothing has been run interactively there.

- **Fonts:** no index of installed fonts (ADR 0005). Non-embedded fonts other than the
  base 14 render with MuPDF's substitutes, and non-embedded CJK text has no glyphs. This
  needs fontconfig (Linux) and Core Text (macOS) behind the existing `SystemFonts` trait.
- **Window:** no Mica or theme tint. A translucent window on macOS needs
  `macOSPrivateApi`, and the title bar's window buttons follow Windows (macOS puts them on
  the left).
- **Touchpad pinch** is wired for WebView2 only (`webview_needs_zoom_controls`); WebKit
  delivers pinches as gesture events, which the viewer does not handle.
- **Platform services:** the default author comes from `USER`. WebView2's memory target
  has no equivalent.
- **Packages** for macOS (dmg, ad-hoc signed, not notarized) and Linux (AppImage and deb,
  plus rpm for final versions; built on Ubuntu 24.04, so glibc 2.39 or later) are built and released (ADR 0011)
  but have never been installed or run. No PDF file association is configured for them.
  Crash recovery, the single-instance hand-off and the in-app updater are
  platform-neutral but untested there.
- **License notices:** `THIRD_PARTY_LICENSES.md` lists the crates of the Windows build
  only; the macOS and Linux builds include more (GTK and WebKitGTK bindings, objc2 and
  others), which it does not name.
- **Tests:** some are Windows-only by design (DirectWrite fonts, locked files, the
  `qpdf.exe` lookup).

## Releases and updates

- **Version:** the first release is `1.0.0-beta.1` (ADR 0011).
- **Installing a downloaded update has not been run.** Checking, the notice, progress,
  Stop, Don't ask again and rejecting a badly signed download were tried against a local
  test server; installing and restarting need a release signed with the real key. The
  first real release is the first full test.
- **The rpm has never been built:** betas leave it out, so the first one comes with
  1.0.0. If that build fails, the 1.0.0 release stops until it is fixed.
- **No code signing** (Authenticode, Apple notarization): SmartScreen and Gatekeeper warn
  on the first manual install (ADR 0011).

## Upstream work

- `docs/upstream/mupdf-1.27.2-jpx-reduced-decoding.patch`: decoding JPEG 2000 at the
  resolution drawn, for Artifex. Not yet sent. Its comments say "Lectrix:"; reword them
  before sending.
- MuPDF's saved journal cannot be loaded back (`pdf_deserialise_journal` and
  `pdf_add_journal_fragment` disagree; ADR 0001). This could be reported.
- After an undo, MuPDF's incremental save writes the trailer's `/Size` from the edit
  section's length, which still counts the objects the undone step created, though it
  writes none of them (`writexref` in `pdf-write.c`). Lectrix trims those entries before
  every incremental save and recovery copy (`ffi::trim_unused_objects`); the fix belongs in
  MuPDF and could be reported.
- The vendored `mupdf` crate's patches (`third_party/mupdf-rs/LECTRIX_PATCHES.md`) could be
  offered to mupdf-rs as a pull request, if the user wants.

## Names kept from the build

- The interop suites keep their build-phase names (`run.py phase0` to `phase5`,
  `phase4-local`, `phase5-local`); CI calls them by these names.
- The `mupdf-rs` fork's branch is `folio-mupdf-1.27.2` and its patch file is
  `folio_patches.rs`, from the app's earlier working name. Renaming them needs a change in
  the fork first.
