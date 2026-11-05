# Releasing: build, sign, notarize, publish, auto-update

Releases are produced by `scripts/release-*.{sh,ps1}` locally or by
`.github/workflows/release.yml` on a `v*` tag, then published to a **separate,
public releases repo** that the in-app updater polls. Keeping the feed public
lets the updater work without auth even when the source repo is private.

```
source repo  ──tag v1.2.0──▶  build + sign  ──▶  OWNER/REPO-releases
                                                   ├─ latest.json        ← updater reads this
                                                   ├─ App.app.tar.gz(.sig)  updater bundle (macOS)
                                                   ├─ App_1.2.0_universal.dmg
                                                   ├─ App_1.2.0_x64-setup.exe(.sig)
                                                   └─ App_1.2.0_amd64.AppImage(.sig), .deb, .rpm
```

## 0. One-time setup

### Updater keypair (all platforms)

```sh
npm run tauri signer generate -- -w ~/.tauri/<slug>.key     # <slug> = package.json name
npm run configure -- --name "<Your App>" --updater-pubkey "$(cat ~/.tauri/<slug>.key.pub)" \
  --releases-repo OWNER/REPO-releases
```

This sets `plugins.updater.pubkey` + `endpoints` and `bundle.createUpdaterArtifacts: true`
in `tauri.conf.json`, and `features.updater: true` in `starter.config.json`.
**Back the private key up (password manager). Never commit it.** If you set a
password on the key, store it in the keychain: `security add-generic-password -s <slug>-updater-key -a <slug> -w`.

Create the public releases repo (empty is fine): `gh repo create OWNER/REPO-releases --public`.

### macOS — Developer ID + notarization

1. Apple Developer Program membership. In Xcode → Settings → Accounts → Manage
   Certificates, create a **Developer ID Application** certificate (it lands in
   your login keychain).
2. Find its identity: `security find-identity -v -p codesigning`. Prefer the
   SHA-1 hash over the name if you have several certs with the same name.
3. App-specific password: appleid.apple.com → Sign-In and Security → App-Specific
   Passwords. Store it: `security add-generic-password -s <slug>-notary -a you@example.com -w`.
4. Hardened Runtime entitlements: `src-tauri/Entitlements.plist` — uncomment only
   what you use (mic, camera). TCC permissions (screen recording, accessibility,
   input monitoring) need **no** entitlement but DO need `NSxxxUsageDescription`
   strings in `src-tauri/Info.plist` for mic/camera; create that file if needed.

### Windows — code signing

Pick one:

- **Azure Trusted Signing** (recommended; no cert files to protect). Create a
  Trusted Signing account + certificate profile in Azure, install a signing CLI
  (`trusted-signing-cli` or `AzureSignTool`), and set in `tauri.conf.json`:
  ```json
  "bundle": { "windows": { "signCommand": "trusted-signing-cli -e https://eus.codesigning.azure.net -a <account> -c <profile> %1" } }
  ```
  Authenticate with `AZURE_TENANT_ID` / `AZURE_CLIENT_ID` / `AZURE_CLIENT_SECRET`.
- **Your own OV/EV certificate**: import the `.pfx` into `CurrentUser\My` and
  pass its thumbprint via `WINDOWS_CERT_THUMBPRINT` (script) or the
  `WINDOWS_CERTIFICATE` + `WINDOWS_CERTIFICATE_PASSWORD` secrets (CI).

Unsigned installers work but trigger SmartScreen warnings until the binary
builds reputation.

### Linux

No central signing. The updater artifact (AppImage) is signed with the updater
key like everywhere else. Optionally GPG-sign bundles (`GPG_SIGN_KEY=<id>`).

## 1. Build locally

```sh
# macOS (universal, signed, notarized, stapled DMG)
APPLE_SIGNING_IDENTITY=<sha1> APPLE_TEAM_ID=<team> scripts/release-macos.sh
SKIP_NOTARIZE=1 … scripts/release-macos.sh        # quick signed-only check

# Windows (PowerShell)
$env:TAURI_SIGNING_PRIVATE_KEY = Get-Content ~/.tauri/<slug>.key -Raw
.\scripts\release-windows.ps1

# Linux
TAURI_SIGNING_PRIVATE_KEY="$(cat ~/.tauri/<slug>.key)" scripts/release-linux.sh
```

Bump the version first in **three** places: `src-tauri/tauri.conf.json`,
`src-tauri/Cargo.toml`, `package.json` (keep them identical; `latest.json` uses
the Tauri one).

Why universal on macOS: a build shell running under Rosetta produces an
x86_64-only binary that never matches an `aarch64` updater entry. Universal
matches every install.

## 2. Publish the feed

```sh
node scripts/publish-feed.mjs --version 1.2.0 --notes "What changed" [--draft] [--dry-run]
```

It finds every bundle + `.sig` under `src-tauri/target`, writes `latest.json`
with one entry per platform present, and creates the GitHub release in the
releases repo with all assets attached. Build on each OS first (or let CI do it).

`latest.json` format (what the updater consumes):

```json
{
  "version": "1.2.0",
  "notes": "…",
  "pub_date": "2026-01-01T00:00:00Z",
  "platforms": {
    "darwin-aarch64": { "signature": "<.sig contents>", "url": "https://github.com/OWNER/REPO-releases/releases/latest/download/App.app.tar.gz" },
    "darwin-x86_64":  { "signature": "…", "url": "…same tarball for universal…" },
    "windows-x86_64": { "signature": "…", "url": "…-setup.exe" },
    "linux-x86_64":   { "signature": "…", "url": "….AppImage" }
  }
}
```

## 3. CI release (tag-triggered)

Push a tag: `git tag v1.2.0 && git push --tags`. `release.yml` builds all three
platforms with `tauri-action`, signs with the secrets below, and creates a
**draft** release in the releases repo including `latest.json`. Review, then
publish the draft — the updater only sees published releases.

Repository secrets:

| Secret | Used for |
| --- | --- |
| `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD` | base64 of the Developer ID `.p12` + its password |
| `APPLE_SIGNING_IDENTITY`, `APPLE_TEAM_ID` | which identity to sign with |
| `APPLE_ID`, `APPLE_PASSWORD` | notarization (app-specific password) |
| `WINDOWS_CERTIFICATE`, `WINDOWS_CERTIFICATE_PASSWORD` | base64 `.pfx` (skip with Azure Trusted Signing) |
| `AZURE_TENANT_ID`, `AZURE_CLIENT_ID`, `AZURE_CLIENT_SECRET` | Azure Trusted Signing |
| `TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | updater artifact signatures |
| `RELEASES_TOKEN` | fine-grained PAT, `contents: write` on the releases repo |

Export the `.p12`: Keychain Access → right-click the Developer ID cert → Export,
then `base64 -i cert.p12 | pbcopy`.

## 4. How updates reach users

1. On launch (and from Settings → Check for updates / the menu) the core fetches
   `latest.json`, compares `version`, verifies the `.sig` with the embedded
   public key, downloads and stages the bundle.
2. It emits `starter://update-ready`; every window shows "Restart to update",
   and a native notification fires so a hidden app still surfaces it.
3. `relaunch()` installs and restarts.

Checks fail closed: a bad signature or a mismatched key is an error, never an
install. The Settings UI distinguishes "up to date" from "couldn't check" from
"updater not configured".

## Troubleshooting

- **"A public key has been found, but no private key"** at build: the updater
  is configured but `TAURI_SIGNING_PRIVATE_KEY` is unset. Harmless for a local
  app bundle; required for release artifacts.
- **Notarization "Invalid"**: run `xcrun notarytool log <id>`; usually a missing
  entitlement or an unsigned nested binary.
- **Updater never fires**: check the `platforms` key matches the running
  binary's target (`darwin-aarch64` vs `darwin-x86_64`), and that the release is
  published, not draft.
- **Gatekeeper warns on the DMG**: Tauri staples the `.app` but not the DMG;
  `release-macos.sh` notarizes and staples the DMG too.
