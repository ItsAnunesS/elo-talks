#!/usr/bin/env bash
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
revision=25e85a4454e93643ad92397d01ed532a360d7ee2
export GOTOOLCHAIN=go1.26.10
source_dir=$(mktemp -d)
trap 'rm -rf "$source_dir"' EXIT
git -C "$source_dir" init -q
git -C "$source_dir" fetch --depth=1 https://github.com/rfjakob/gocryptfs.git "$revision"
git -C "$source_dir" checkout --detach FETCH_HEAD
mkdir -p "$root/src-tauri/helpers"
# Upstream 2.6.1 still pins old x/crypto. Use fixed, audited dependency versions.
(cd "$source_dir" && go get golang.org/x/crypto@v0.58.0 golang.org/x/sys@v0.49.0 golang.org/x/term@v0.47.0)
(cd "$source_dir" && CGO_ENABLED=0 go test -tags without_openssl ./internal/cryptocore ./internal/contentenc ./internal/nametransform)
(cd "$source_dir" && CGO_ENABLED=0 go build -trimpath -tags without_openssl -ldflags '-s -w' -o "$root/src-tauri/helpers/gocryptfs-x86_64-unknown-linux-gnu" .)
cp "$root/src-tauri/helpers/gocryptfs-x86_64-unknown-linux-gnu" "$root/src-tauri/helpers/gocryptfs"
cp "$source_dir/LICENSE" "$root/src-tauri/helpers/gocryptfs-LICENSE"
(cd "$source_dir" && go list -m -json all) > "$root/src-tauri/helpers/gocryptfs-modules.json"

(cd "$source_dir" && CGO_ENABLED=0 go run golang.org/x/vuln/cmd/govulncheck@v1.9.0 -tags without_openssl .)

# WebKit's sandbox launch paths are rewritten relative to AppDir/usr by linuxdeploy.
# Ship both helpers rather than asking users to install them.
for helper in bwrap xdg-dbus-proxy; do
  cp "/usr/bin/$helper" "$root/src-tauri/helpers/$helper-x86_64-unknown-linux-gnu"
done

cp "$root/src-tauri/helpers/bwrap-x86_64-unknown-linux-gnu" "$root/src-tauri/helpers/bwrap-real-x86_64-unknown-linux-gnu"
cp "$root/scripts/bwrap.sh" "$root/src-tauri/helpers/bwrap-x86_64-unknown-linux-gnu"
