# Planned: translation

Part of the status: [docs/status.md](../status.md) is the index a session reads, with one line for each item; this file has the plan in full. Keep both current.

Decided 2026-11-01, not started. Lectrix's interface is translated by its community on
Hosted Weblate. Only the frontend's text is translated; the Rust code's text stays in
English.

## Scope

- **Translated:** the text of the frontend (`src/`): components, dialogs, panels, the
  menus (the macOS menu bar takes its labels from the frontend, `src-tauri/src/menu.rs`),
  tooltips (`data-tip`), accessible names, the settings schema, shortcut names, the start
  screen and the notifications the frontend makes. Dates and numbers already go through
  `Intl` with `LOCALE` (`src/lib/config.ts`), which becomes the chosen language.
- **Stays in English:** text made in Rust: the errors and their suggestions (`AppError`,
  about 60 messages), the file dialogs' filter name ("PDF documents"), the log. The NSIS
  installer, README, docs, specs and `CHANGELOG.md` are not in Weblate either; What's New
  already shows a `CHANGELOG_XX.md` translation when one exists (`changelog.ts`).
- **Later, not in this plan:** right-to-left languages (Arabic, Hebrew), which mirror the
  layout; translating Rust's errors (they would go to the frontend as codes).

## Hosting: Hosted Weblate, Libre plan

- Free for public libre projects, with the limits of the 160k plan; hosted strings are
  source strings x (languages + 1), so about 2,000 strings leave room for about 79
  languages. Stated in one sentence under the price table on
  [weblate.org/en/hosting](https://weblate.org/en/hosting/), not as a plan of its own.
- Register on hosted.weblate.org, set the project up (a 14-day trial starts), and ask for
  the Libre plan on the first day: in 2023 a project's request went unanswered until it
  was warned of removal at the trial's end.
- Fallback: Weblate is GPL, so it can be self-hosted at no cost.

## How it works

- The English file in the repo is the only one developers edit. Weblate follows the repo
  (a GitHub webhook), offers new and changed strings to translators, and sends their work
  back as a pull request from its own branch, which the maintainer merges. A string not
  yet translated shows in English.
- Before a release, merge Weblate's pending pull request (a step in `docs/releasing.md`).
- README gets a "Help translate" section with Weblate's progress badge.

## Code work, in order

1. **Choose the file format first:** Paraglide JS (inlang), Svelte's official i18n
   library, compiles each message to a typed function (strict TypeScript catches a
   missing or misnamed one) and drops unused ones from the build. Check before building
   that its files carry plural forms Weblate's plural editor understands (i18next JSON v4
   with CLDR suffixes is Weblate's recommended JSON format; inlang has an i18next plugin),
   since Russian and Polish need three or four forms. Check its license (AGPL-compatible)
   and size for the commit message and `THIRD_PARTY_NOTICES.md`.
2. **An ADR** for the library, the file format, the key naming (by feature, as
   `annotations.tools.highlight`) and the English-only Rust text; AGENTS.md's stack table
   gets the library.
3. **A Language setting** (section 6.6: a field in Rust's `Settings`, a row in
   `schema.ts`), following the system's language by default. A change applies at once
   in every window without reloading the page (Paraglide's default `setLocale` reloads
   it, which would lose what is open), so the text has to be reactive.
4. **Move the strings feature by feature**, one commit each, with no visible change in
   English, so the e2e tests keep passing as they are.
5. **Checks:** a test that components hold no hard-coded user-visible text (as the
   `data-tip` test does), and a pseudo-language in the Debug menu (accented and about 35%
   longer, as German runs) to find text that is cut off or not moved.
6. **Docs:** the spec of each feature where its text rules change, `docs/spec/ui.md` (text
   must fit longer languages), `docs/releasing.md`, `docs/manual-checklist.md` (one pass
   in a translated language), README.

## Settled by the user (2026-11-01)

- **A new language applies at once**, in every window, with no restart; nothing open is
  lost (documents, panels, tools, scroll positions).
- **Every language with translations ships**, however incomplete; what is missing shows in
  English.
- **Russian is the first language**, translated by the maintainer.
