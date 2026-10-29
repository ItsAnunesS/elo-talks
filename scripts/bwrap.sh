#!/bin/bash
# WebKit's AppImage relocation uses paths relative to AppDir/usr. A sandbox
# changes the filesystem view: normalize those paths and expose only the
# read-only application bundle, keeping WebKit's seccomp/namespaces unchanged.
set -euo pipefail
: "${APPDIR:?AppImage directory is required}"
[[ "$APPDIR" = /* && -x "$APPDIR/usr/bin/bwrap-real" ]] || exit 125
expanded=()
while (($#)); do
  if [[ "$1" == --args ]]; then
    fd="$2"
    [[ "$fd" =~ ^[0-9]+$ ]] || exit 125
    items=()
    mapfile -d '' -t items <&"$fd"
    expanded+=("${items[@]}")
    shift 2
  else
    expanded+=("$1")
    shift
  fi
done
args=()
separator=false
for arg in "${expanded[@]}"; do
  if [[ "$arg" == -- && "$separator" == false ]]; then
    args+=(--ro-bind /usr /usr --ro-bind "$APPDIR" "$APPDIR")
    separator=true
  elif [[ "$arg" == ./* ]]; then
    while [[ "$arg" == ./* ]]; do arg="${arg#./}"; done
    arg="$APPDIR/usr/${arg#/}"
  fi
  args+=("$arg")
done
[[ "$separator" == true ]] || exit 125
exec "$APPDIR/usr/bin/bwrap-real" "${args[@]}"
