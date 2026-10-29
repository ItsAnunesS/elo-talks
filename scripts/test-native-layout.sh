#!/usr/bin/env bash
# CI-only accommodation for Ubuntu's restricted unprivileged user namespaces.
# This profile is never packaged or installed by the application.
set -euo pipefail
cd "$(dirname "$0")/.."
binary="$(realpath target/debug/examples/layout_smoke)"
profile='elo-talks-ci-layout'
profile_file="$(mktemp)"
loaded=false
cleanup() {
  if [[ "$loaded" == true ]]; then
    sudo apparmor_parser -R "$profile_file"
  fi
  rm -f "$profile_file"
}
trap cleanup EXIT

if [[ "$(cat /proc/sys/kernel/apparmor_restrict_unprivileged_userns 2>/dev/null || true)" == 1 ]]; then
  [[ "${GITHUB_ACTIONS:-}" == true ]] || {
    echo 'Run this CI-only test on GitHub Actions; no host security settings were changed.' >&2
    exit 1
  }
  # Only this exact synthetic test executable gains userns permission. Neither
  # the global kernel restriction nor the WebKit sandbox is disabled.
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

for scale in 1 2; do
  xvfb-run -a -s '-screen 0 3200x2000x24' \
    env -u WEBKIT_DISABLE_SANDBOX_THIS_IS_DANGEROUS \
    GDK_SCALE="$scale" WEBKIT_FORCE_SANDBOX=1 LIBGL_ALWAYS_SOFTWARE=1 \
    dbus-run-session -- timeout 30s "$binary"
done
