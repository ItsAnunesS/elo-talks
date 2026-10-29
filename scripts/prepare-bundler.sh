#!/usr/bin/env bash
set -euo pipefail
cache="${XDG_CACHE_HOME:-$HOME/.cache}/tauri"
# Tauri patches its linuxdeploy copy in place. Cache pristine downloads in a
# separate directory and verify every restored file before staging a fresh copy.
pristine="${XDG_CACHE_HOME:-$HOME/.cache}/elo-talks/bundler"
mkdir -p "$cache" "$pristine"
fetch() {
  local name="$1" checksum="$2" url="$3"
  if ! printf '%s  %s\n' "$checksum" "$pristine/$name" | sha256sum --check --status 2>/dev/null; then
    local download
    download=$(mktemp "$pristine/.download.XXXXXX")
    if ! curl --fail --location --proto '=https' --tlsv1.2 "$url" -o "$download"; then
      rm -f "$download"
      return 1
    fi
    if ! printf '%s  %s\n' "$checksum" "$download" | sha256sum --check --strict; then
      rm -f "$download"
      return 1
    fi
    mv "$download" "$pristine/$name"
  fi
  cp "$pristine/$name" "$cache/$name"
  chmod +x "$cache/$name"
}

fetch AppRun-x86_64 f30140a43a0a59e46db21bdefdf749b9e9f2c6946e92afabbacf98b8ae73fb4f https://github.com/tauri-apps/binary-releases/releases/download/apprun-old/AppRun-x86_64
fetch linuxdeploy-07333c6-x86_64.AppImage 36a2d7e274d12e1050d0e9ecfe11d339ed54720b2bec464c286d53f8b07f5c62 https://github.com/tauri-apps/binary-releases/releases/download/linuxdeploy-07333c6/linuxdeploy-x86_64.AppImage
fetch linuxdeploy-plugin-appimage.AppImage 49d6a17160675a6bd1781699aae6bdf7692d98552e02a3671d2183d10547842e https://github.com/linuxdeploy/linuxdeploy-plugin-appimage/releases/download/continuous/linuxdeploy-plugin-appimage-x86_64.AppImage
