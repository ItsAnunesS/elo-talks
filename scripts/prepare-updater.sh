#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../updater"
export GOTOOLCHAIN=go1.27.3 CGO_ENABLED=0
ELO_UPDATER_NETWORK_TEST=1 go test -mod=readonly ./...
go run golang.org/x/vuln/cmd/govulncheck@v1.9.0 ./...
go build -mod=readonly -trimpath -ldflags '-s -w' -o ../src-tauri/helpers/elo-updater-x86_64-unknown-linux-gnu .
cp ../src-tauri/helpers/elo-updater-x86_64-unknown-linux-gnu ../src-tauri/helpers/elo-updater
go list -m -json all > ../src-tauri/helpers/elo-updater-modules.json
