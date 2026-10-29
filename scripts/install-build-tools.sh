#!/usr/bin/env bash
set -euo pipefail
# The binaries are cached separately from application builds. Version mismatches
# are rebuilt rather than silently accepting an incompatible cached tool.
audit_version="$(cargo audit --version 2>/dev/null || true)"
if [[ "$audit_version" != 'cargo-audit 0.22.2' && "$audit_version" != 'cargo-audit-audit 0.22.2' ]]; then
  cargo install cargo-audit --version 0.22.2 --locked --force
fi
if [[ "$(cargo tauri --version 2>/dev/null || true)" != 'tauri-cli 2.12.4' ]]; then
  cargo install tauri-cli --version 2.12.4 --locked --force
fi
