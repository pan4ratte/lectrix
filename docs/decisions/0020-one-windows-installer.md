# 0020: Windows has one installer; the MSI goes

- Status: accepted (the user's decision, 2026-10-13)
- Date: 2026-10-13

## Context

Lectrix shipped two Windows installers since Phase 6: the NSIS setup program
(`Lectrix_<version>_x64-setup.exe`) and an MSI. They were told apart by where they
installed: NSIS per user, the MSI for everyone, into Program Files. ADR 0015 then gave
the NSIS installer both ("just me" or "everyone"), and from there the MSI did nothing the
setup program does not do: it installs for everyone, runs silently (`/S /AllUsers`), and
has the same "PDF files" page.

What the MSI still cost:

- a WiX fragment for the PDF page and its registry entries, kept equal to the NSIS
  template's by hand (ADR 0007);
- `msi-version.mjs` and `npm run bundle`: MSI versions are numbers only, so every
  pre-release had to end in a number and plain `tauri build` failed on one (ADR 0011);
- a second install, registration and uninstall pass in CI and on the manual checklist,
  and a second update path on Windows (`msiexec /i` beside the setup program's `/UPDATE`);
- a choice on the download page that needed explaining, and whose explanation ("the
  `.msi`, to install for every user") had stopped being true.

Its one remaining use is deployment tools that take nothing but an MSI (Group Policy
software installation). Nobody has asked for that. The user asked whether every
installer should stay, and decided to drop the MSI.

## Decision

1. **The Windows build makes the NSIS installer only** (`bundle.targets` in
   `src-tauri/tauri.windows.conf.json`). The WiX fragment, the `wix` settings and
   `msi-version.mjs` are gone; `npm run bundle` is `tauri build`, kept as the one command
   the docs and CI name. The release has one Windows file, and `latest.json` no
   `windows-x86_64-msi` entry.
2. **Copies installed from the MSI keep updating.** 1.1.0 is the last version with an
   MSI. Such a copy asks `latest.json` for `windows-x86_64-msi`, finds none, and takes
   `windows-x86_64`, the setup program (`get_urls` in tauri-plugin-updater 2.13.1 looks
   for `{os}-{arch}-{installer}`, then `{os}-{arch}`). It tells an installer from an MSI
   by the file's content, so it runs the download as the setup program: `/P /UPDATE`.
3. **The setup program takes the MSI install over.** Tauri's template already does most
   of this: its reinstall step finds a Windows Installer product named Lectrix by the
   publisher `lectrix`, runs its uninstall command, and then creates the shortcuts again
   although it is an update. Three things are added (`src-tauri/windows/installer.nsi`,
   marked `Lectrix:`):
   - **No question.** The uninstall command (`MsiExec.exe /X{product code}`) asks "Are
     you sure you want to uninstall this product?", which a user who pressed Restart now
     to update should never see, and answering No would leave the update half done. In a
     passive install it gets `/passive /norestart`: a progress bar and nothing to answer.
   - **The PDF choice is kept.** The MSI takes its registration with it, and an update
     reads the earlier choice from the registry (ADR 0011), where it would find nothing
     and leave Lectrix unregistered. So the choice is read before the MSI is removed.
     Run by hand, the "PDF files" page starts unchecked if the MSI was not registered.
   - **Lectrix is closed first.** The template checks for a running Lectrix only in its
     Install section, after the MSI is gone; Windows Installer may put off deleting a
     running program's files until the next restart, when they would be the new ones.
   The folder stays `C:\Program Files\Lectrix` and the registration under `HKLM`: an
   administrator's install without an NSIS one before it is per machine (ADR 0015).
4. **Tested in CI** (`tests/installer/check.ps1`): the 1.1.0 MSI is downloaded from its
   release (its SHA-256 is pinned in the script), installed with and without the PDF
   registration, and updated with `/P /UPDATE`. The MSI product must be gone, Lectrix
   must be in Program Files with the NSIS uninstall entry, the registration must be as it
   was, and the uninstaller must leave nothing.
5. **Pre-releases no longer have to end in a number.** Nothing derives a version from the
   last part any more. They are still written `1.2.0-beta.1`, so a second beta can
   follow the first.

## Consequences

- One Windows download, one installer to test, and no second copy of the PDF
  registration to keep in step.
- An MSI user's first update after 1.1.0 shows two progress windows, Windows Installer's
  and then the setup program's, and puts a shortcut on the desktop again if it had been
  deleted (the template's rule for a WiX migration). Later updates are ordinary ones.
- Group Policy software installation, which takes MSI packages only, cannot deploy
  Lectrix. Intune, Configuration Manager and scripts can run
  `Lectrix_<version>_x64-setup.exe /S /AllUsers` (with `/NOPDF` to leave the registration
  out).
- ADR 0007's MSI half and ADR 0011's point 7 (the MSI version) describe what was.

## Known limits

- **A silent install (`/S`) does not look for an MSI install**: NSIS shows no pages
  then, and the template looks on a page. Run over an MSI install, it installs into the
  same folder and leaves both in Settings > Apps. Remove the MSI first
  (`msiexec /x Lectrix_1.1.0_x64_en-US.msi /qn`). The in-app updater is passive, not
  silent, and is not affected.
- **An MSI install in a folder of the user's choosing moves to Program Files** at its
  first update. The shortcuts and the PDF registration follow.
- **A user without administrator rights cannot remove an MSI install.** Windows asks for
  an administrator's password during the update; without it the MSI stays, and the setup
  program installs for that user beside it. The MSI's own updates needed an
  administrator too.
- The takeover while Lectrix is still running, and an update started from inside an
  MSI-installed 1.1.0, are not run by CI, which starts the setup program itself. The
  second is on the manual checklist.

## Alternatives considered

- **Keep both.** No migration, but every cost above stays for a package that adds
  nothing since ADR 0015.
- **Keep publishing the MSI as it is, without maintaining it.** The costs are in building
  and testing it, not in listing it; an untested installer is worse than none.
- **Drop it and leave MSI installs as they are.** The updater's fallback would still run
  the setup program over them, unasked: with the template as Tauri has it, that means a
  confirmation prompt in the middle of an update and a lost PDF registration.
- **Stop updating MSI installs** (a `windows-x86_64-msi` entry that never changes, or a
  check in the app). Those copies would stay on 1.1.0 for good, without being told.
- **A last MSI release that tells its users to switch.** One more release with both
  installers, and the same takeover to write afterwards anyway.
