# Releasing Lectrix

How releases and in-app updates work is recorded in ADR 0011. This page covers what you do.

## Once: the updater key

The in-app updater only installs files signed with Lectrix's updater key.

1. Create the key pair and choose a password:

   ```
   npx tauri signer generate -w "$HOME/.tauri/lectrix.key"
   ```

   This writes `lectrix.key` (private) and `lectrix.key.pub` (public). Write `$HOME`, not
   `~`: PowerShell passes `~` to the command unchanged, and the key then lands in a folder
   named `~` inside the current directory, which may be this repository.
2. Put the contents of `lectrix.key.pub` into `src-tauri/tauri.conf.json`, as
   `plugins.updater.pubkey`, and commit it.
3. In the GitHub repository, go to Settings > Secrets and variables > Actions and add:
   - `TAURI_SIGNING_PRIVATE_KEY`: the contents of `lectrix.key`;
   - `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`: its password.
4. **Back up the private key and its password** somewhere outside this computer. Without
   them, installed copies of Lectrix will refuse every later update.

## Each release

1. In `CHANGELOG.md`, rename `## Unreleased` to `## <version>` (below).
2. Bump `"version"` in `package.json` (and `package-lock.json`, for example with
   `npm version 1.0.0-beta.2 --no-git-tag-version`) and push to `main`. Pre-releases
   end in a number (`1.0.0-beta.2`, `1.0.0-rc.1`), so the next one can follow. After the
   betas comes `1.0.0`. The `/release` command does this step
   and the commit: it takes the version from the first `## <version>` in `CHANGELOG.md`.
3. CI runs. It fails if `CHANGELOG.md` has no heading for the version in `package.json`.
   When it passes, the Release workflow sees that the version has no release yet. It
   builds every platform, attests the installers and publishes the release with
   `latest.json`; the release's description is that version's part of `CHANGELOG.md`.
   Betas are published as ordinary releases (the updater skips GitHub pre-releases).
4. Copies of Lectrix that start after that offer the update. After updating, the first
   start shows a notification that opens What's new.

A run that fails leaves a draft release. Fix the problem, then run the workflow again
(Actions > Release > Run workflow): it replaces the draft.

To build the installers on your own machine, run `npm run bundle`. On Windows that is the
setup program alone: there is no MSI since 1.1.0 (ADR 0020).

## The changelog

`CHANGELOG.md`, at the repository root, says what changed in each version, for the
people who use Lectrix. Help > What's new shows it, and the Release workflow writes each
GitHub release's description from it. `latest.json` carries the latest ten versions' part,
so the update notice can show what an update brings before it is installed.

**When.** With every change people will notice, add a line under `## Unreleased` at the
top, in the commit that makes the change. Leave out what they can't notice (refactoring,
tests, CI). At release time, `## Unreleased` becomes `## <version>`: the Release workflow
fails if changes are still listed under `## Unreleased`, and CI fails if the version in
`package.json` has no heading. A pre-release has its own heading (`## 1.1.0-beta.1`);
when the final version follows, its heading lists everything since the last final
version, the betas' changes included.

**Shape.**

```markdown
## 1.1.0

An optional sentence or two about the main news.

### Viewer and tabs

- **View > Page display** can show two pages side by side.
- Pages appear sooner while scrolling.

### General

- Lectrix starts faster.
```

Under each version, the sections with changes, in this order (`SECTIONS` in
`src/lib/features/changelog/changelog.ts`; CI checks it):

| Section | What goes there |
| --- | --- |
| Start screen | the screen with no document open: tools, recent files |
| Viewer and tabs | pages, zoom, scrolling, search, text, tabs, thumbnails, the side panes |
| Annotations | the tools, the annotation list, comments and replies, repair |
| Bookmarks | the Bookmarks panel and its inspector |
| Page labels | the Page labels dialog |
| Combining files | the Combine view, Insert pages from file |
| Organizing pages | the grid of a document's pages: moving, rotating, deleting, extracting |
| Compressing files | the Compress dialog |
| Printing | the Print dialog |
| Comparing documents | the Compare view: the documents compared, the options, the differences, the report |
| Settings | the Settings dialog |
| Installation and updates | installers, file association, the updater |
| General | what touches the whole app: speed, stability, the look, keyboard, accessibility |

A new tool gets a section of its own, before Settings: add it to `SECTIONS` and to this
table.

**Wording.**

- One line per change, saying what people can do or see now, not how it was done: "Pages
  appear sooner while scrolling", not "Cached display lists per page".
- Plain, short sentences. Name controls as the app shows them, menu paths in bold:
  **Help > What's new**. A fix says what works now: "Undo restores a deleted bookmark's
  children."
- Bold is the only formatting: no links, code, images, sub-headings or nested lists (the
  app shows none of them; CI checks it). No issue numbers or internal names.

To read the next release's notes as users will, run the app in development (`npx tauri
dev`) and pick **Debug > Update notice > Update available**, then See what's new: it
offers the next version with what is under Unreleased.

**Translations.** A translation is a file next to it named after the language:
`CHANGELOG_RU.md`, `CHANGELOG_PT-BR.md`. It mirrors `CHANGELOG.md`: the same version
headings (`## Unreleased` may be in its own words), the same sections in the same order
with headings in its language, the same number of changes in each. It may lack versions:
What's new shows a version the translation doesn't have in English. The app picks the file
for its language (`LOCALE` in `src/lib/config.ts`, until the app itself is translated),
then for the base language (`pt-BR`, then `pt`). CI checks the mirroring. GitHub releases
are in English.

## Checking a download

Each installer has a build provenance attestation:

```
gh attestation verify Lectrix_1.0.0-beta.1_x64-setup.exe -R pan4ratte/lectrix
```

The release has no `.sig` files: the updater's signatures are in `latest.json`.

## What users see on first install

The builds are not code-signed by Microsoft or Apple (ADR 0011):

- **Windows:** SmartScreen may say "Windows protected your PC". Choose More info, then Run
  anyway.
- **macOS:** the first launch is blocked. Allow it in System Settings > Privacy &
  Security (Open Anyway).

Updates installed from inside Lectrix show neither warning.
