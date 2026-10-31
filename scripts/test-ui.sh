#!/usr/bin/env bash
# Synthetic browser tests; this temporary CI profile never ships in the AppImage.
set -euo pipefail
cd "$(dirname "$0")/.."
binary="$(node -p 'require("playwright").chromium.executablePath()')"
profile='elo-talks-ci-browser'
profile_file="$(mktemp)"
loaded=false
cleanup() {
  if [[ "$loaded" == true ]]; then sudo apparmor_parser -R "$profile_file"; fi
  rm -f "$profile_file"
}
trap cleanup EXIT
if [[ "$(cat /proc/sys/kernel/apparmor_restrict_unprivileged_userns 2>/dev/null || true)" == 1 ]]; then
  [[ "${GITHUB_ACTIONS:-}" == true ]] || exit 1
  [[ "$binary" != *'"'* && "$binary" != *$'\n'* ]] || exit 1
  cat > "$profile_file" <<PROFILE
abi <abi/4.0>,
include <tunables/global>
profile $profile "$binary" flags=(unconfined) {
  userns,
}
PROFILE
  sudo apparmor_parser -r "$profile_file"
  loaded=true
fi
CHROME_BIN="$binary" timeout 180s node tests/ui.spec.cjs
