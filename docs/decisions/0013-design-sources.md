# 0013: Design sources live in the app repo, under `design/`

- Status: accepted (the user's decision, 2026-10-06)
- Date: 2026-10-06

## Context

The Lectrix brand kit (the Swan icon, 2026-10-05) is a zip with the master artwork and
its exports for every platform. The app already uses parts of it: the app icons in
`src-tauri/icons/`, the icon in the About dialog (`src/lib/assets/lectrix-icon.svg`), the
Google Sans build (ADR 0009) and the accent colours (ADR 0010). The masters these came
from, the wordmark, the lockup, the PDF file icon and the Yaru versions were nowhere in
the repo, so changing an icon or adding the PDF file icon meant finding the zip again.

AGENTS.md section 4 fixes the repository layout and asks for an ADR to change it.

## Decision

1. **A top-level `design/` folder** holds the kit's sources: the brand specs
   (`README.md`), `tokens.json`, the kit's `lectrix.css` for reference, the app icon's
   master SVGs and Icon Composer layers (`app-icon/`), the wordmark, the lockup, the PDF
   file icon with its Windows `.ico` (`file-icon/`) and the Yaru versions (`yaru/`).
2. **Nothing in `design/` is built or shipped as it is.** The app keeps its own copies
   where it uses them (`src-tauri/icons/`, `src/lib/assets/`) and its own design tokens
   in `src/app.css`. A file goes into the app by being copied out of `design/`, and the
   change that does it says so.
3. **Platform exports are left out**: the kit's `png/`, `windows/`, `macos/`, `linux/`
   and `web/` folders are generated from the masters, and the app's set is in
   `src-tauri/icons/`. So are the full font builds (`brand/fonts/`, 1.6 MB each), which
   the app does not use, and the kit's `index.html` overview.
4. **Licences.** Lectrix's own artwork in `design/` is all rights reserved, not
   AGPL-3.0-or-later (the user's decision; `design/README.md`). The Yaru versions keep
   their own licence, CC BY-SA 4.0, with its notice in `design/yaru/LICENSE.md` and a
   line in `THIRD_PARTY_NOTICES.md`. They are not installed by the Linux packages:
   `application-pdf` is the icon theme's own icon for every PDF, and the app icon would
   be written into another package's theme folder.

## Alternatives considered

- **A separate design repo.** Keeps binary artwork out of the app's history, but an icon
  change would then span two repos, and the masters would drift from what the app ships.
  The sources are about 1 MB.
- **Keeping only the zip** (in a release or outside git). Nothing to maintain in the
  repo, but files can't be diffed, reviewed or linked from docs, and the zip has to be
  found and unpacked for every change.
