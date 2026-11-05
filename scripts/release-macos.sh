#!/usr/bin/env bash
# Build a signed + notarized macOS release: universal (arm64 + x86_64) .app,
# DMG, and — when the updater is configured — the signed updater tarball.
#
# Secrets come from the environment or the macOS Keychain; nothing is stored in
# the repo. One-time setup (SLUG = package.json "name", e.g. acme-notes):
#
#   # Developer ID Application certificate must be in the login keychain.
#   # Apple ID + app-specific password (appleid.apple.com → App-Specific Passwords):
#   security add-generic-password -s <SLUG>-notary -a "you@example.com" -w
#   # Updater keypair (once; keep the private key OUT of the repo):
#   npm run tauri signer generate -- -w ~/.tauri/<SLUG>.key
#   #   → paste the public key into tauri.conf.json plugins.updater.pubkey
#   #     (or: npm run configure -- --name … --updater-pubkey "$(cat ~/.tauri/<SLUG>.key.pub)")
#   # If you protected the key with a password:
#   security add-generic-password -s <SLUG>-updater-key -a <SLUG> -w
#
# Required env (or edit the defaults below):
#   APPLE_SIGNING_IDENTITY  SHA-1 or name of "Developer ID Application: …"
#   APPLE_TEAM_ID           10-char team id
#
# Usage: scripts/release-macos.sh                 # signed + notarized
#        SKIP_NOTARIZE=1 scripts/release-macos.sh # signed only (fast local check)
set -euo pipefail
cd "$(dirname "$0")/.."

SLUG="$(node -p "require('./package.json').name")"
: "${APPLE_SIGNING_IDENTITY:?set APPLE_SIGNING_IDENTITY (security find-identity -v -p codesigning)}"
: "${APPLE_TEAM_ID:?set APPLE_TEAM_ID}"
UPDATER_KEY="${UPDATER_KEY:-$HOME/.tauri/$SLUG.key}"

kc() { security find-generic-password -s "$1" -w 2>/dev/null; }
kc_account() { security find-generic-password -s "$1" 2>/dev/null | sed -n 's/.*"acct"<blob>="\(.*\)"/\1/p'; }

export APPLE_SIGNING_IDENTITY APPLE_TEAM_ID

if [[ -z "${SKIP_NOTARIZE:-}" ]]; then
  APPLE_ID="${APPLE_ID:-$(kc_account "$SLUG-notary")}"
  APPLE_PASSWORD="${APPLE_PASSWORD:-$(kc "$SLUG-notary")}"
  if [[ -z "$APPLE_ID" || -z "$APPLE_PASSWORD" ]]; then
    echo "error: no Apple ID credentials. Set APPLE_ID/APPLE_PASSWORD or add Keychain item '$SLUG-notary'." >&2
    exit 1
  fi
  export APPLE_ID APPLE_PASSWORD
  echo "→ notarization: enabled (${APPLE_ID%%@*}@…, team $APPLE_TEAM_ID)"
else
  echo "→ notarization: SKIPPED"
fi

if [[ -f "$UPDATER_KEY" ]]; then
  TAURI_SIGNING_PRIVATE_KEY="$(cat "$UPDATER_KEY")"
  TAURI_SIGNING_PRIVATE_KEY_PASSWORD="$(kc "$SLUG-updater-key" || true)"
  export TAURI_SIGNING_PRIVATE_KEY TAURI_SIGNING_PRIVATE_KEY_PASSWORD
  echo "→ updater artifacts: signed with $UPDATER_KEY"
else
  echo "→ updater artifacts: no key at $UPDATER_KEY (fine if the updater is disabled)"
fi

# Universal: a build shell running under Rosetta silently produces x86_64-only
# otherwise, which then never matches an aarch64 updater feed.
rustup target add aarch64-apple-darwin x86_64-apple-darwin >/dev/null
BUNDLE_DIR="src-tauri/target/universal-apple-darwin/release/bundle"
npm run tauri build -- --target universal-apple-darwin --bundles app,dmg

# Tauri notarizes/staples the .app but not the DMG; Gatekeeper checks the DMG.
if [[ -z "${SKIP_NOTARIZE:-}" ]]; then
  for dmg in "$BUNDLE_DIR"/dmg/*.dmg; do
    echo "→ notarizing $(basename "$dmg")"
    xcrun notarytool submit "$dmg" --apple-id "$APPLE_ID" --password "$APPLE_PASSWORD" \
      --team-id "$APPLE_TEAM_ID" --wait | grep -E "status:" | tail -1
    xcrun stapler staple "$dmg" | tail -1
    spctl -a -vv -t open --context context:primary-signature "$dmg" 2>&1 | head -2
  done
fi

echo; echo "Artifacts:"
ls -1 "$BUNDLE_DIR"/dmg/*.dmg "$BUNDLE_DIR"/macos/*.tar.gz* 2>/dev/null || true
echo; echo "Publish: node scripts/publish-feed.mjs --version <x.y.z> --notes '…'"
