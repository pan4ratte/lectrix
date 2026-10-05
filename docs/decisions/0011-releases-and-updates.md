# 0011: Releases on GitHub, and an in-app updater as the only network access

- Status: accepted (the user's decision, 2026-10-05)
- Date: 2026-10-05

## Context

AGENTS.md section 2 had a hard constraint: "Offline: no network access at runtime, no
telemetry, no update checks." The user asked for:

1. a release workflow, triggered by a version bump in `package.json`, that builds the app
   for Windows, Linux and macOS and attests the artifacts;
2. an update system: check GitHub releases in the background at every start; a floating
   notice offering "Don't ask again", "Not now" and "Update"; on Update, a download with a
   progress bar and an install that needs nothing more from the user; then an offer to
   restart.

The user chose (2026-10-05):

- to allow network access **for updates only**, with a Settings switch;
- "Don't ask again" skips **that version**: a later release is offered;
- signing: Tauri's updater signature and GitHub artifact attestation only. No Windows
  Authenticode or Apple notarization for now.

## Decision

1. **The only network access** is the update check and download, from Rust through
   `tauri-plugin-updater` (2.13.1). The check asks
   `https://github.com/pan4ratte/lectrix/releases/latest/download/latest.json`; the
   request carries the plugin's user agent and nothing about the user or their files. The
   webview stays offline: its CSP is unchanged, and it has no updater permission. It
   calls Lectrix's own commands in `src-tauri/src/update.rs`.
2. **When it checks:** once per start, after the startup documents are open. Not when
   Settings > "Check for updates when Lectrix starts" is off, not in debug builds and not
   with `LECTRIX_EPHEMERAL` (measurement and test runs). `LECTRIX_UPDATES=0` or `1` turns
   checks off or forces them on. Failures (offline, GitHub down) are logged and show
   nothing.
3. **The notice** floats above the notifications, bottom right. It never takes focus and
   is announced politely. "Not now" hides it until the next start. "Don't ask again"
   stores that version in app data (`skipped_update`), and only that version is skipped.
4. **Updating** downloads with progress (Stop cancels it). The plugin verifies the
   download against the public key in `tauri.conf.json` before anything is installed.
   - **macOS, Linux:** the new version is installed at once. The notice says so and offers
     Restart now or Later; the new version runs from the next start either way.
   - **Windows:** a program cannot replace itself while it runs, so the verified
     installer is kept in memory. Restart now asks about unsaved documents (as closing the
     window does), then runs the installer in passive mode (a small progress window, no
     questions) and the installer opens Lectrix again. Later hides the notice, and the
     installer runs when Lectrix closes, without reopening it. This is as close as
     Windows allows to "installed, then offered a restart".
   - An NSIS update runs the installer with `/UPDATE`. It now keeps the earlier PDF-file
     choice (ADR 0007): before this change, a passive update would have registered Lectrix
     for PDFs even if the user had declined. MSI upgrades keep feature states already.
5. **The app's version** comes from `package.json` (`tauri.conf.json` `"version":
   "../package.json"`), so a bump there is the whole release step. The Cargo workspace
   version is not used by the app.
6. **Release workflow** (`.github/workflows/release.yml`): it runs after CI passes on
   `main`, or by hand. If `v<version>` has no tag yet, it:
   - creates a draft release;
   - builds Windows x64 (NSIS and MSI), Linux x64 (AppImage, deb) and macOS on Apple
     Silicon and Intel (dmg and the updater's `.app.tar.gz`), with
     `src-tauri/tauri.release.conf.json` turning on the signed update bundles;
   - attests every installer with `actions/attest-build-provenance`;
   - writes `latest.json` itself (`.github/scripts/release-assets.mjs`), so parallel
     builds cannot overwrite each other's entries;
   - publishes the release, which creates the tag.

   The updater key lives in the `TAURI_SIGNING_PRIVATE_KEY` and
   `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` secrets (`docs/releasing.md`). Ordinary CI builds
   don't need it: they don't make update bundles.
7. **Versions and pre-releases.** Lectrix starts at `1.0.0-beta.1` (the user's choice,
   2026-10-05). A pre-release must end in a number, because MSI versions are numbers
   only: `npm run bundle` (`.github/scripts/msi-version.mjs`) gives the MSI
   `1.0.0-beta.N` as `1.0.0.N` and `1.0.0` as `1.0.0`. Windows Installer compares only the
   first three fields, and Tauri's MSI allows same-version upgrades, so 1.0.0 replaces its
   betas. Plain `tauri build` fails on a pre-release version for that reason; CI, the
   Release workflow and local installer builds use `npm run bundle`. Betas are published
   as ordinary releases, not GitHub pre-releases, because the updater reads
   `releases/latest`, which skips pre-releases.
8. **Linux packages** are the AppImage (which the updater replaces) and a deb, plus an
   rpm for final versions only: an rpm version cannot contain `-`, and a pre-release
   version does (`src-tauri/tauri.linux.conf.json` leaves rpm out; the Release workflow
   adds it when the version has no pre-release part).
9. **macOS builds are ad-hoc signed** (`signingIdentity: "-"`), which Apple Silicon
   requires for the app to run at all. They are not notarized.

## Consequences

- Lectrix contacts github.com once per start unless the user turns that off. The About
  dialog still shows the source address instead of opening it.
- Losing the private key or its password means installed copies refuse every later
  update: users would have to reinstall by hand. Back both up.
- Without Authenticode, SmartScreen warns when people first run a downloaded installer.
  Without notarization, macOS blocks the first launch until it is allowed in System
  Settings > Privacy & Security. Updates installed by the app are not affected. Either
  kind of signing can be added to the workflow later without changing the updater.
- Linux and macOS builds are released but only built and unit-tested in CI. They have no
  end-to-end or interop runs (docs/status.md, platform gaps).
- The Linux build is made on Ubuntu 24.04, so its AppImage needs glibc 2.39 or later.
- New dependencies: `tauri-plugin-updater` and what it brings (reqwest, rustls with ring,
  rustls-platform-verifier for the system's certificate store, minisign-verify, zip and
  tar), all MIT, Apache-2.0, ISC or BSD.

## Alternatives considered

- **`tauri-apps/tauri-action`** for building and uploading: convenient, but its
  `latest.json` is merged by each matrix job in turn, and parallel jobs can overwrite each
  other's entries. Writing the manifest once, after every build, is deterministic.
- **Installing on Windows straight after the download:** the app would close without
  warning, possibly in the middle of work. Asking first matches how closing the window
  behaves.
- **The updater's JavaScript API** (`@tauri-apps/plugin-updater`): it would give the
  webview network-capable permissions. Rust commands keep the webview's surface as small
  as it was.
- **Checking periodically while the app runs:** not requested, and every request is one
  more reason for the app to go online.
- **Triggering on every push that changes `package.json`, without waiting for CI:** it
  could release a commit that fails tests.
