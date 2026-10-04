# 0007: Installers ask whether Folio opens PDF files, and never take over the default

- Status: proposed (Phase 6)
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
   - a ProgID `Folio.Document` (description, icon, `shell\open\command` = `"…\folio.exe" "%1"`,
     which the app already handles: command line and single-instance hand-off, Phase 1);
   - `.pdf\OpenWithProgids\Folio.Document`;
   - `Software\Folio\Capabilities` (name, description, `FileAssociations\.pdf`) and
     `RegisteredApplications\Folio`, so Folio appears in Default apps.

   They never write the default value of `.pdf` or any `UserChoice` key, and uninstalling
   removes exactly these entries. The NSIS installer installs per user (Tauri's default),
   so it writes under `HKCU`; the MSI installs per machine, under `HKLM`.
2. **A page asks.** Both installers show a "PDF files" page after the folder page, with
   "Open PDF files with Folio" (checked by default) and a sentence saying that Windows
   will ask which app to use. Silent and passive installs register Folio unless told not
   to: `/NOPDF` for NSIS, `FOLIO_ASSOCIATE_PDF=0` for the MSI.
3. **How, in each installer:**
   - **NSIS:** a copy of Tauri 2.12.1's template (`src-tauri/windows/installer.nsi`)
     with the page, the `/NOPDF` option and the registry writes added. Tauri's
     installer hooks cannot add a page in the middle (the hook file is included before
     the first page), so the template itself has to change. Every change is marked
     `Folio:`, and the header says where the original came from.
   - **MSI:** Tauri's own `main.wxs` is left alone. A WiX fragment
     (`src-tauri/windows/pdf-association.wxs`, `bundle.windows.wix.fragmentPaths`)
     defines the registry entries as an optional feature and the page as a dialog
     between the folder page and the last page; its Next button adds or removes the
     feature.
4. **Tested in CI** (`tests/installer/check.ps1`, after the installers are built): each
   installer is installed silently with and without the registration, the registry and
   install folder are checked, it is uninstalled, and everything must be gone. The
   default value of `.pdf` must be unchanged throughout.

## Consequences

- After installing, the next PDF the user opens brings up Windows' own "How do you want
  to open this file?" with Folio offered. That is the supported path to becoming the
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
  at the next PDF. It could be added to Folio's own Settings dialog later.
