# Releases and updates

Part of the status: [docs/status.md](../status.md) is the index a session reads, with one line for each item; this file has the release state in full. Keep both current.

- **Version:** `1.2.0`, after `1.1.0` (ADR 0011).
- **Installing a downloaded update has not been run.** Checking, the notice, progress,
  Stop, Don't ask again and rejecting a badly signed download were tried against a local
  test server; installing and restarting need a release signed with the real key. The
  first real release is the first full test.
- **No code signing** (Authenticode, Apple notarization): SmartScreen and Gatekeeper warn
  on the first manual install (ADR 0011).
  - **Plan for Windows** (decided 2026-10-12, not started): sign through SignPath
    Foundation, which signs open-source projects for free with its own certificate, so
    the publisher shown is "SignPath Foundation". Before applying: ask them whether the
    app icon and the PDF file icon, which are all rights reserved
    (`THIRD_PARTY_NOTICES.md`), break their rule against proprietary components, and
    publish the code signing policy they require. Then the Release workflow signs the app
    and its installer. A signature names the publisher; SmartScreen's warning still
    fades only as downloads build reputation. Not chosen: Certum's open-source
    certificate (from 25 EUR, in the author's name, its key on a card or in their cloud)
    and Azure Artifact Signing (individuals in the USA and Canada only).
  - **macOS: nothing decided.** The only route is the Apple Developer Program (99 USD a
    year) for a Developer ID certificate and notarization.
  - The README's "Download and install" says the builds are not signed; change it when
    either is done.
- **Package managers** (decided 2026-10-19, not started): winget and Flathub, no others.
  - **winget:** a manifest for the NSIS installer in `microsoft/winget-pkgs`
    (`wingetcreate new`), with two installers, per user (`/S /CurrentUser`) and per machine
    (`/S /AllUsers`), and a step in the Release workflow that files each version's pull
    request (`winget-releaser` or `komac`, with a token in the repository's secrets).
    Unsigned installers are accepted; the in-app updater stays on.
  - **Flathub:** a flatpak-builder manifest on `org.gnome.Platform` (WebKitGTK 4.1), kept
    in its own repository under the `flathub` organisation once accepted. To decide first:
    the app ID, which must be on a domain Lectrix controls (`io.github.pan4ratte.Lectrix`
    unless `lectrix.org` is ours; the Flatpak's desktop file and metainfo are named after
    it); a grant that lets the icons, now all rights reserved, be redistributed unmodified;
    and a build switch that leaves out the update check, since Flathub forbids apps that
    update themselves (an ADR, and `docs/spec/settings.md`). Reviewers expect an offline
    build from source: cargo and npm sources vendored (`flatpak-cargo-generator`,
    `flatpak-node-generator`), the MuPDF fork, NASM and libclang from the SDK extensions.
    The sandbox needs `--filesystem=home` (or `host`): a save writes a temporary file beside
    the target and renames it, recent files are reopened and open files watched, and the
    file portal allows none of these. Without the update check it needs no network; links
    go through the OpenURI portal. The metainfo needs screenshots, `<releases>` and a
    content rating, and `flatpak-builder-lint` must pass.
- **Administrators answer a UAC prompt on every Windows update**, per-user installs
  included: the NSIS installer offers "just me" or "everyone" and asks for the highest
  rights when it starts (ADR 0015). Only users without administrator rights update with
  no prompt. Updating `1.0.0-beta.1`, which installed per user without recording it,
  keeps it in `%LOCALAPPDATA%\Lectrix`.
- **The MSI is gone after 1.1.0** (ADR 0020, 2026-10-13). A copy installed from it
  updates through the setup program, which removes the MSI install first. CI's installer
  check runs the takeover on the released 1.1.0 MSI, with and without the PDF
  registration, and it passed in every run of 2026-10-13. Not yet done: an update started
  from inside an MSI-installed 1.1.0, now that 1.2.0 is out
  (`docs/manual-checklist.md`). Its limits (a silent install over an MSI, an MSI in a
  folder of the user's choosing, a user without administrator rights) are in the ADR.
- **Tests that failed only on CI, or only sometimes** (2026-10-13: 1.2.0 took five tries on CI).
  The first run failed four end-to-end tests that had only ever run on the author's
  screen: CI's runners ask for reduced motion and their window is 1028 px wide. The
  next three each failed one test that passes most of the time (a click that closes the
  window before the driver answers; a wheel over a page the grid has no cell for; a
  Rust test writing a file a session had closed a moment before), and once GitHub gave
  the macOS job no runner. All were faults of the tests. Since then:
  `npm run test:ci --prefix tests/e2e` shows the app as CI does (it fails the four tests
  as they were, with CI's messages), the tests run beside an open Lectrix, the soak run
  lists the tests that fail only sometimes (`.github/workflows/soak.yml`), and CI's
  Windows checks are separate jobs, so one that failed is run again alone. A click that
  makes Lectrix quit goes through `clickToQuit` (`tests/e2e/lib/app.mjs`), since the
  nightly soak of 2026-10-22 caught `recovery.test.mjs`'s "Don't save" losing that race
  once in ten passes. Known and left: `visual-adjustments.test.mjs`'s full-screen test
  fails about half its runs on the author's screen in `test:ci` mode ("Esc left full
  screen": the title bar is back after Esc closes the adjustments panel; 2026-10-18,
  with and without that day's change to how pages are shown).

## What v1 contains

v1 was completed on 2026-10-04: all seven build phases were finished, reviewed by the
user and passed CI.

Everything in AGENTS.md sections 1 and 6: the viewer with tabs, bookmarks, page labels,
combining files and inserting pages, the seven annotation types with an annotation list,
and repair. Also: undo and redo for every edit, atomic and incremental saving, crash
recovery, recent files with each file's last view, Settings (author name; System, Light
or Dark appearance), an About dialog with a button to the AGPL source on GitHub, and an installer
that can register Lectrix for PDF files (ADR 0007) and installs for the user or for
everyone (ADR 0015). Up to 1.1.0 there was an MSI as well (ADR 0020).

Since v1: releases on GitHub for Windows, Linux and macOS, built by the Release
workflow with attested installers, and an in-app updater with a Settings switch
(ADR 0011). Replies to annotations, written in the annotation list (ADR 0012). Scroll
modes (single page, two pages, each continuous or not), with a Settings default. A
changelog (`CHANGELOG.md`), shown by Help > What's new, before and after an update, and
used as each release's description. Update checks once a day. Files that claim PDF/A
open for reading only until Enable editing, with a Settings switch, and lose the claim
when a save may have broken it. Several windows: a tab moves into a window of its own or
into another window, with its document as it is. Full screen (F11): the pages alone, with
the toolbar floating and the side panes brought by the pointer or by Alt held. Visual
adjustments (brightness, contrast, sepia, inverted colours, greyscale) for a document's pages,
on screen only and remembered per file. The pages next to those on screen are rendered
ahead, so that pages slow to render (scans) are there when scrolled to. Organizing pages
(AGENTS.md section 6.7, ADR 0018): a document's pages as a grid in the viewer's place,
where they are moved, rotated, deleted, inserted from other files and extracted into a
new document, which is not a file until it is saved (ADR 0019), and where the page labels
are set in a dialog, in place of their side panel. The start screen has it among its
tools. Rotating in the viewer turns a page on screen only, remembered per file. Unlock, Protect,
Document properties and Compress (sections 6.8 to 6.11), and Print (section 6.12, ADR 0028).
