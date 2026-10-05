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

1. Bump `"version"` in `package.json` (and `package-lock.json`, for example with
   `npm version 1.0.0-beta.2 --no-git-tag-version`) and push to `main`. Pre-releases must
   end in a number (`1.0.0-beta.2`, `1.0.0-rc.1`): the MSI version is made from it
   (ADR 0011). After the betas comes `1.0.0`.
2. CI runs. When it passes, the Release workflow sees that the version has no release yet.
   It builds every platform, attests the installers and publishes the release with
   `latest.json`. Betas are published as ordinary releases (the updater skips GitHub
   pre-releases).
3. Copies of Lectrix that start after that offer the update.

A run that fails leaves a draft release. Fix the problem, then run the workflow again
(Actions > Release > Run workflow): it replaces the draft.

To build the installers on your own machine, run `npm run bundle` (plain
`npx tauri build` fails for a beta version: the MSI needs a numeric version).

## Checking a download

Each installer has a build provenance attestation:

```
gh attestation verify Lectrix_1.0.0-beta.1_x64-setup.exe -R pan4ratte/lectrix
```

## What users see on first install

The builds are not code-signed by Microsoft or Apple (ADR 0011):

- **Windows:** SmartScreen may say "Windows protected your PC". Choose More info, then Run
  anyway.
- **macOS:** the first launch is blocked. Allow it in System Settings > Privacy &
  Security (Open Anyway).

Updates installed from inside Lectrix show neither warning.
