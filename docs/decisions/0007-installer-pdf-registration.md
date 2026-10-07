# 0007: Installers ask whether Lectrix opens PDF files, and never take over the default

- Status: accepted (Phase 6 review, 2026-10-04)
- Date: 2026-10-04

## Context

Phase 6 asks for NSIS and MSI installers in which "the `.pdf` file association can be
chosen at install" (AGENTS.md section 10). Two facts shape this:

1. **Tauri's bundler can only associate unconditionally.** `bundle.fileAssociations`
   makes both installers register the extension on every install, with no choice. Its
   NSIS macro (`APP_ASSOCIATE`) also overwrites the default value of `.pdf`.
2. **Since Windows 8, no installer can make itself the default app for a file type.**
   The default lives in a per-user `UserChoice` key protected by a hash only Windows
   writes; Windows 10 and 11 ignore (and sometimes reset) programs that write it
   themselves. What an installer can do is *register* the app: then Windows lists it in
   Open with and in Settings > Apps > Default apps, and asks the user which app to use
   the next time they open a PDF ("new app available").

## Decision

1. **Register, don't take over.** When the user chooses it, the installers write:
   - a ProgID `Lectrix.Document` (description, icon, `shell\open\command` = `"…\lectrix.exe" "%1"`,
     which the app already handles: command line and single-instance hand-off, Phase 1).
     The icon is the Lectrix PDF file icon (point 5);
   - `.pdf\OpenWithProgids\Lectrix.Document`;
   - `Software\Lectrix\Capabilities` (name, description, `FileAssociations\.pdf`) and
     `RegisteredApplications\Lectrix`, so Lectrix appears in Default apps.

   They never write the default value of `.pdf` or any `UserChoice` key, and uninstalling
   removes exactly these entries. They go under `HKCU` for a per-user install and `HKLM`
   for a per-machine one: the NSIS installer does either (ADR 0015; per user only before
   it), the MSI installs per machine.
2. **A page asks.** Both installers show a "PDF files" page after the folder page, with
   "Open PDF files with Lectrix" (checked by default) and a sentence saying that Windows
   will ask which app to use. Silent and passive installs register Lectrix unless told not
   to: `/NOPDF` for NSIS, `LECTRIX_ASSOCIATE_PDF=0` for the MSI.
3. **How, in each installer:**
   - **NSIS:** a copy of Tauri 2.12.1's template (`src-tauri/windows/installer.nsi`)
     with the page, the `/NOPDF` option and the registry writes added. Tauri's
     installer hooks cannot add a page in the middle (the hook file is included before
     the first page), so the template itself has to change. Every change is marked
     `Lectrix:`, and the header says where the original came from.
   - **MSI:** Tauri's own `main.wxs` is left alone. A WiX fragment
     (`src-tauri/windows/pdf-association.wxs`, `bundle.windows.wix.fragmentPaths`)
     defines the registry entries as an optional feature and the page as a dialog
     between the folder page and the last page; its Next button adds or removes the
     feature.
4. **Tested in CI** (`tests/installer/check.ps1`, after the installers are built): each
   installer is installed silently with and without the registration, the registry and
   install folder are checked, it is uninstalled, and everything must be gone. The
   default value of `.pdf` must be unchanged throughout.
5. **PDFs get the Lectrix PDF icon** (added 2026-10-06; until then `DefaultIcon` was
   `lectrix.exe,0`, the app icon). The brand kit's `lectrix-pdf.ico` (`design/file-icon/`,
   ADR 0013) is copied to `src-tauri/icons/lectrix-pdf.ico` and installed next to
   `lectrix.exe` as a resource. That resource is set in `src-tauri/tauri.windows.conf.json`,
   so the macOS and Linux packages don't carry it. `DefaultIcon` is
   `"…\lectrix-pdf.ico",0` in both installers; an in-app update rewrites the registration,
   so it reaches existing installs. CI checks that `DefaultIcon` names that file, that the
   file exists, and that uninstalling removes it. Explorer shows the icon for PDFs only
   while Lectrix is the default app for them.

   Embedding the icon in `lectrix.exe` as a second icon resource was not chosen.
   `tauri-build` already compiles the exe's one resource file, so a second icon would need
   another resource compiler step in `build.rs` (or a new dependency). `DefaultIcon` would
   also have to name it by index or resource ID, which depends on how the icons are
   ordered in the exe. A separate file needs no build change, and both bundlers already
   install and remove resources.

## Consequences

- After installing, the next PDF the user opens brings up Windows' own "How do you want
  to open this file?" with Lectrix offered. That is the supported path to becoming the
  default on Windows 10 and 11.
- Upgrading the Tauri CLI means re-applying the marked NSIS changes to its new template
  (a few dozen lines). The MSI fragment depends only on WixUI's standard dialog names
  (`InstallDirDlg`, `VerifyReadyDlg`), which Tauri's template uses as well.
- The interactive pages themselves are not exercised by CI (only silent installs are);
  they are on the manual checklist.

## Alternatives considered

- **`bundle.fileAssociations`:** no choice at install, and the NSIS macro overwrites the
  `.pdf` default value (with a backup), which Windows ignores for users who have a
  `UserChoice` anyway.
- **A Yes/No message box from an NSIS hook after installing:** no template fork, but an
  odd interruption at the end of the install and no equivalent in the MSI.
- **WiX's feature tree (`WixUI_FeatureTree`) for the MSI:** shows Tauri's internal
  features (shortcuts, PATH) as well, and needs a forked `main.wxs`.
- **Opening Settings > Default apps after installing:** Windows 11 can deep-link there,
  but sending people to Settings during an install is heavier than letting Windows ask
  at the next PDF. It could be added to Lectrix's own Settings dialog later.
