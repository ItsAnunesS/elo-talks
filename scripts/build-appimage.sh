#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
# Standard AppImage metadata recognized by Gear Lever and AppImageUpdate.
export LDAI_UPDATE_INFORMATION='gh-releases-zsync|ItsAnunesS|elo-talks|latest|*amd64.AppImage.zsync'
# Compatibility with the older linuxdeploy interface used by Tauri.
export UPDATE_INFORMATION="$LDAI_UPDATE_INFORMATION"
export LDAI_VERSION
LDAI_VERSION="$(python3 -c 'import json; print(json.load(open("src-tauri/tauri.conf.json"))["version"])')"
export VERSION="$LDAI_VERSION"
if [[ "$VERSION" == *-rc.* ]]; then
  export LDAI_UPDATE_INFORMATION="gh-releases-zsync|ItsAnunesS|elo-talks|v$VERSION|*amd64.AppImage.zsync"
  export UPDATE_INFORMATION="$LDAI_UPDATE_INFORMATION"
fi
cargo tauri build --bundles appimage -- --locked
# Tauri may leave the generated companion outside its bundle directory.
# Generate it at a predictable path from the final image before signing.
for image in target/release/bundle/appimage/*.AppImage; do
  zsyncmake -u "$(basename "$image")" -o "$image.zsync" "$image"
done
python3 scripts/verify-appimage-update.py target/release/bundle/appimage
