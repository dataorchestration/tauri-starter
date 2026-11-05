#!/usr/bin/env bash
# Build Linux release bundles: AppImage (what the updater uses), .deb and .rpm.
#
# Build deps (Debian/Ubuntu):
#   sudo apt install libwebkit2gtk-4.1-dev build-essential curl wget file \
#     libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev patchelf
# Fedora: webkit2gtk4.1-devel openssl-devel libappindicator-gtk3-devel librsvg2-devel patchelf
#
# Linux has no central code-signing authority; what you CAN do:
#   * sign the updater artifacts (TAURI_SIGNING_PRIVATE_KEY, same key as other OSes)
#   * optionally GPG-sign the bundles for people who verify downloads
#     (GPG_SIGN_KEY=<key id> → produces .asc files next to each bundle)
#
# Usage: scripts/release-linux.sh
set -euo pipefail
cd "$(dirname "$0")/.."

if [[ -n "${TAURI_SIGNING_PRIVATE_KEY:-}" ]]; then
  echo "→ updater artifacts: signed"
else
  echo "→ updater artifacts: not signed (fine if the updater is disabled)"
fi

npm run tauri build -- --bundles appimage,deb,rpm

BUNDLE_DIR="src-tauri/target/release/bundle"
echo; echo "Artifacts:"
ls -1 "$BUNDLE_DIR"/appimage/*.AppImage* "$BUNDLE_DIR"/deb/*.deb "$BUNDLE_DIR"/rpm/*.rpm 2>/dev/null || true

if [[ -n "${GPG_SIGN_KEY:-}" ]]; then
  for f in "$BUNDLE_DIR"/appimage/*.AppImage "$BUNDLE_DIR"/deb/*.deb "$BUNDLE_DIR"/rpm/*.rpm; do
    [[ -f "$f" ]] || continue
    gpg --batch --yes --local-user "$GPG_SIGN_KEY" --armor --detach-sign "$f"
    echo "  signed $(basename "$f") → $(basename "$f").asc"
  done
fi
echo; echo "Publish: node scripts/publish-feed.mjs --version <x.y.z> --notes '…'"
