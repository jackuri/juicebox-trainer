# Releasing OpenTrainer

## Make a release

1. Bump the version everywhere: `npm run bump -- X.Y.Z`. Add a `## [X.Y.Z] - date` entry to
   `CHANGELOG.md`; its text becomes the release notes.
2. `npm run check` must pass.
3. Commit, then push a tag `vX.Y.Z`. The **Release** workflow (`.github/workflows/release.yml`)
   builds the installer on Windows and publishes a GitHub release with:
   - `OpenTrainer_X.Y.Z_x64-setup.exe`: the installer (NSIS, per-user, no admin rights)
   - `SHA256SUMS.txt`
   - `latest.json` and `…setup.exe.sig`, when the updater key is set (see below)

The workflow refuses a tag that doesn't match the version in `package.json`.

To build the installer locally:

```powershell
npm run build:agent
node scripts/release-config.mjs
npm run tauri --workspace @opentrainer/desktop -- build --config ../../release.conf.json
node scripts/release-files.mjs      # collects everything in release-out/
```

## What's in the installer

- `opentrainer-desktop.exe` (the app, which is also the AI helper's tool server with `--mcp`).
- `ai-agent\*.js` and `ai-agent\package.json`: the AI helper's own scripts, plus the exact engine
  versions it needs. The engine itself (Anthropic's Claude Agent SDK, about 330 MB, not under
  OpenTrainer's licence) is **not** shipped. The app downloads it from npm when the player clicks
  "Download the AI engine". It runs `npm install` with exactly those versions and
  `--ignore-scripts`, into `%LOCALAPPDATA%\org.opentrainer.desktop\ai-helper`. Node.js must be
  installed.
- A file association for `.otpack`. Double-clicking one opens the running OpenTrainer, or starts it.

## Code signing (not set up yet)

Unsigned installers make Windows show "Windows protected your PC". The release workflow signs
automatically once a certificate is available. Add these two repository secrets
(**Settings → Secrets and variables → Actions**):

| Secret | Value |
|---|---|
| `WINDOWS_CERTIFICATE` | the code-signing certificate (`.pfx`), base64-encoded |
| `WINDOWS_CERTIFICATE_PASSWORD` | its password |

Ways to get one:
- **SignPath Foundation**: free for open-source projects, but the repository must be public. They
  sign through their own GitHub action, so the workflow's signing step would change to call it.
- **Microsoft Trusted Signing** (Azure): about $10 a month, with identity checks. Also signs through
  its own action or CLI rather than a `.pfx`.
- A classic OV/EV certificate from a certificate authority, as a `.pfx`: works with the secrets above
  as they are. EV certificates on hardware tokens need a different setup.

Never add antivirus evasion (packers, obfuscation) instead. Signing is the only fix.

## Updates

The app uses Tauri's updater. It reads
`https://github.com/jackuri/opentrainer/releases/latest/download/latest.json` and installs an update
only if it was signed with OpenTrainer's updater key. The public key is in
`apps/desktop/src-tauri/tauri.conf.json` (`plugins.updater.pubkey`). The updater also checks that the
signed version matches the announced one (`requireSignedVersion`), so old releases can't be replayed
as new ones.

**The private updater key** was generated on the maintainer's PC and is kept outside the repository,
in `%USERPROFILE%\.opentrainer-release-keys\updater.key` (no password). Back it up somewhere safe.
If it's lost, installed copies can't be updated any more; they'd need a manual reinstall with a
new key. To let the workflow sign update files, add it as a secret:

| Secret | Value |
|---|---|
| `TAURI_SIGNING_PRIVATE_KEY` | the contents of `updater.key` |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | empty (the key has no password) |

In-app updates are switched on only in builds made by the release workflow **on a public
repository with that secret set** (the build gets `OT_UPDATES=1`). Every other build says
"Automatic updates are off in this copy". Release assets of a private repository can't be
downloaded without signing in, so updates can't work before the repository is public.

To try the updater locally, serve a `latest.json` and start a debug build with
`OT_UPDATE_URL=http://127.0.0.1:<port>/latest.json`. The update must still be signed with the real
key. Local `http://` addresses also need `"dangerousInsecureTransportProtocol": true` under
`plugins.updater` in a `--config` file, for that test only.

## Community pack list

The Packs tab reads `index.json` from the public repository `jackuri/opentrainer-packs`
(`ot_packs::community::INDEX_URL`). Its layout:

```text
index.json                  made with: otpack index packs index.json
packs/<game-id>/pack.json   one folder per game, named by the pack's game.id
```

The app downloads `index.json` (at most 2 MB), then a pack's `pack.json` (at most 4 MB). It checks
the file against the SHA-256 in the index and validates it like any other pack before showing or
installing it. The rules for accepting packs are in that repository's README. Single-player games
only, no game files, and the pack must pass `otpack validate`.
