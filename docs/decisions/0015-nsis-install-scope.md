# 0015: The NSIS installer asks "just me" or "everyone"

- Status: accepted (the user's decision, 2026-10-07)
- Date: 2026-10-07

## Context

The NSIS installer (`Lectrix_<version>_x64-setup.exe`) installed per user, into
`%LOCALAPPDATA%\Lectrix`. Nobody had chosen this: `bundle.windows.nsis.installMode` was
not set, and Tauri's default is `currentUser`. The MSI installs per machine, into
Program Files. The user asked why Lectrix does not install into Program Files, the usual
place.

Tauri's NSIS template supports three modes:

- `currentUser`: no administrator rights needed to install or update; the in-app updater
  (ADR 0011) installs without any prompt. Only the installing account gets Lectrix, and
  some managed PCs block programs that run from AppData.
- `perMachine`: Program Files, for every account; a UAC prompt on every install and
  every update.
- `both`: a page asks "Install for anyone using this computer" or "Install just for me".
  Tauri builds it on NSIS's `MultiUser.nsh` with `MULTIUSER_EXECUTIONLEVEL Highest`, so
  the installer asks for the highest rights the user has as soon as it starts, before
  the page.

## Decision

1. **`installMode: "both"`**, as Tauri builds it (`src-tauri/tauri.conf.json`). The user
   chose it knowing what it costs:
   - Administrators (most home users) get a UAC prompt when the installer starts,
     whichever they then pick, and so on **every in-app update**. ADR 0011's "an install
     that needs nothing more from the user" holds only for users without administrator
     rights.
   - For an administrator, "everyone" is preselected (MultiUser.nsh's default); a
     standard user gets "just me", with "everyone" unavailable.
   - "Just me" installs into `%LOCALAPPDATA%\Programs\Lectrix` (Windows' per-user program
     folder), "everyone" into `C:\Program Files\Lectrix`.
   - An update keeps the earlier mode: the installer records it in the uninstall key
     (`CurrentUser` or `AllUsers`), and the updater's passive install skips the page.
   - Lectrix still starts without administrator rights after an install or update
     (`nsis_tauri_utils::RunAsUser`), so files dragged from Explorer still drop.
   - The PDF registration (ADR 0007) goes under `HKCU` or `HKLM` to match.
2. **Silent installs** take `/CurrentUser` or `/AllUsers`; without either, an
   administrator's install is per machine and a standard user's per user.
3. **Older per-user installs stay where they are.** Installs from before this decision
   did not record their mode, so an administrator's update would have defaulted to
   "everyone" and put a second copy in Program Files. The template's `.onInit` keeps
   the per-user mode when a per-user install exists and no per-machine one does, unless
   `/AllUsers` is given; the existing folder (`%LOCALAPPDATA%\Lectrix`) is kept too.
4. `tests/installer/check.ps1` installs, updates and uninstalls both ways, and
   simulates an older per-user install being updated.

## Alternatives considered

- **Keep `currentUser`, and write it down.** No UAC prompts at all, and the per-machine
  MSI is there for those who want Program Files. This was recommended; the user preferred
  to offer the choice in the NSIS installer too.
- **`perMachine`.** Program Files always, with a UAC prompt on every install and update,
  even for a user who would have been happy with a per-user install.
- **`both` with "just me" preselected** (`MULTIUSER_INSTALLMODE_DEFAULT_CURRENTUSER`, one
  line in the template). Offered; the user kept Tauri's default.
- **`both` without a prompt for per-user installs**: start without administrator rights
  and ask for them only when "everyone" is picked, by replacing Tauri's install-mode
  handling with our own or with the third-party NsisMultiUser plugin. No UAC prompt on a
  per-user update, but the most work and another dependency in the installer.

## Known limits

- Choosing "everyone" on the page while a per-user install exists (or the other way
  round) installs a second copy beside the first, as Tauri's template does; uninstall the
  other one from Settings > Apps.
- Per-user installs from before this decision keep `%LOCALAPPDATA%\Lectrix`; new ones go
  to `%LOCALAPPDATA%\Programs\Lectrix`.
