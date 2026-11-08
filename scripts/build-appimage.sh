#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
export LDAI_UPDATE_INFORMATION='gh-releases-zsync|ItsAnunesS|elo-talks|latest|*amd64.AppImage.zsync'
export UPDATE_INFORMATION="$LDAI_UPDATE_INFORMATION"
export LDAI_VERSION
LDAI_VERSION="$(python3 -c 'import json; print(json.load(open("src-tauri/tauri.conf.json"))["version"])')"
export VERSION="$LDAI_VERSION"
if [[ "$VERSION" == *-rc.* ]]; then
  export LDAI_UPDATE_INFORMATION="gh-releases-zsync|ItsAnunesS|elo-talks|v$VERSION|*amd64.AppImage.zsync"
  export UPDATE_INFORMATION="$LDAI_UPDATE_INFORMATION"
fi
: "${CEF_PATH:?Run scripts/prepare-cef.py and export CEF_PATH first}"
# Use the existing checksum-pinned bundler. The alpha CEF bundler assumes removed
# ANGLE files and installs a runtime hook that offers to modify host AppArmor.
cargo tauri build --no-bundle -- --locked
build_root="$(realpath "${CARGO_TARGET_DIR:-target}")"
output="$build_root/release/bundle/appimage"
mkdir -p "$output"
app_dir="$(mktemp -d "$build_root/cef-appdir.XXXXXX")"
app_run="$app_dir.apprun"
trap 'rm -rf "$app_dir"; rm -f "$app_run"' EXIT
install -Dm755 "$build_root/release/elo-talks" "$app_dir/usr/bin/elo-talks"
for helper in gocryptfs elo-updater; do
  install -Dm755 "src-tauri/helpers/$helper-x86_64-unknown-linux-gnu" "$app_dir/usr/bin/$helper"
done
for resource in icudtl.dat v8_context_snapshot.bin chrome_100_percent.pak chrome_200_percent.pak resources.pak vk_swiftshader_icd.json; do
  install -Dm644 "$CEF_PATH/$resource" "$app_dir/usr/bin/$resource"
done
mkdir -p "$app_dir/usr/bin/locales" "$app_dir/usr/lib"
cp "$CEF_PATH"/locales/en-US*.pak "$app_dir/usr/bin/locales/"
# CEF resolves ICU and resource packs beside its loaded shared library. linuxdeploy
# places libcef in usr/lib, so expose the adjacent data there as well.
for resource in icudtl.dat v8_context_snapshot.bin chrome_100_percent.pak chrome_200_percent.pak resources.pak vk_swiftshader_icd.json locales; do
  ln -s "../bin/$resource" "$app_dir/usr/lib/$resource"
done
# Never install a setuid helper or a sandbox-bypass wrapper. CEF requires the
# host's unprivileged user namespaces and fails closed if they are unavailable.
install -Dm644 "$CEF_PATH/LICENSE.txt" "$app_dir/usr/share/licenses/cef/LICENSE.txt"
install -Dm644 "$CEF_PATH/CREDITS.html" "$app_dir/usr/share/licenses/cef/CREDITS.html"
for license in LICENSE-MIT LICENSE-APACHE-2.0; do
  install -Dm644 "vendor/tauri-runtime-cef/$license" "$app_dir/usr/share/licenses/tauri-runtime-cef/$license"
done
install -Dm644 vendor/phf_generator/LICENSE "$app_dir/usr/share/licenses/phf_generator/LICENSE"
install -Dm644 src-tauri/helpers/gocryptfs-LICENSE "$app_dir/usr/share/licenses/gocryptfs/LICENSE"
install -Dm644 src-tauri/icons/icon.png "$app_dir/usr/share/icons/hicolor/512x512/apps/elo-talks.png"
mkdir -p "$app_dir/usr/share/applications"
cat > "$app_dir/usr/share/applications/dev.anuness.elotalks.desktop" <<'DESKTOP'
[Desktop Entry]
Type=Application
Name=Elo Talks
Exec=elo-talks
Icon=elo-talks
Categories=Network;InstantMessaging;
Terminal=false
StartupWMClass=dev.anuness.elotalks
DESKTOP
cat > "$app_run" <<'APPRUN'
#!/bin/sh
set -eu
APPDIR="${APPDIR:-$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)}"
export APPDIR
export LD_LIBRARY_PATH="$APPDIR/usr/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
export XDG_DATA_DIRS="$APPDIR/usr/share:${XDG_DATA_DIRS:-/usr/local/share:/usr/share}"
export GDK_BACKEND=x11
exec "$APPDIR/usr/bin/elo-talks" "$@"
APPRUN
chmod 755 "$app_run"
libraries=()
for library in libcef.so libvk_swiftshader.so libvulkan.so.1; do
  libraries+=(--library "$CEF_PATH/$library")
  ln -s "../lib/$library" "$app_dir/usr/bin/$library"
done
# Older compatible distributions may include separate ANGLE libraries.
for library in libEGL.so libGLESv2.so; do
  if [[ -f "$CEF_PATH/$library" ]]; then
    libraries+=(--library "$CEF_PATH/$library")
    ln -s "../lib/$library" "$app_dir/usr/bin/$library"
  fi
done
# NSS loads these modules dynamically; ldd alone cannot discover them.
for library in libsoftokn3.so libfreeblpriv3.so libnssckbi.so; do
  resolved="$(find /usr/lib -name "$library" -print -quit)"
  [[ -n "$resolved" ]] || { echo "Missing NSS module: $library" >&2; exit 1; }
  libraries+=(--library "$resolved")
done
cache="${XDG_CACHE_HOME:-$HOME/.cache}/tauri"
export PATH="$cache:$PATH"
export APPIMAGE_EXTRACT_AND_RUN=1
export OUTPUT="$output/Elo Talks_${VERSION}_amd64.AppImage"
LD_LIBRARY_PATH="$CEF_PATH${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}" \
"$cache/linuxdeploy-07333c6-x86_64.AppImage" --appdir "$app_dir" \
  --executable "$app_dir/usr/bin/elo-talks" --executable "$app_dir/usr/bin/gocryptfs" \
  --executable "$app_dir/usr/bin/elo-updater" "${libraries[@]}" \
  --desktop-file "$app_dir/usr/share/applications/dev.anuness.elotalks.desktop" \
  --icon-file "$app_dir/usr/share/icons/hicolor/512x512/apps/elo-talks.png" \
  --custom-apprun "$app_run" --output appimage
zsyncmake -u "$(basename "$OUTPUT")" -o "$OUTPUT.zsync" "$OUTPUT"
python3 scripts/verify-appimage-update.py "$output"
