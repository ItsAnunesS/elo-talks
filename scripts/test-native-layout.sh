#!/usr/bin/env bash
# CI-only accommodation for Ubuntu's restricted unprivileged user namespaces.
# This profile is never packaged or installed by the application.
set -euo pipefail
cd "$(dirname "$0")/.."
example="${1:-layout_smoke}"
case "$example" in
  layout_smoke|clipboard_smoke|cef_notification_smoke|shell_smoke|media_permission_smoke) ;;
  *) echo 'Unsupported native fixture.' >&2; exit 1 ;;
esac
: "${CEF_PATH:?Prepare the pinned CEF distribution first}"
stage="$(mktemp -d "${CARGO_TARGET_DIR:-target}/native-$example.XXXXXX")"
cp "${CARGO_TARGET_DIR:-target}/debug/examples/$example" "$stage/$example"
for resource in "$CEF_PATH"/*; do ln -s "$resource" "$stage/$(basename "$resource")"; done
binary="$(realpath "$stage/$example")"
profile="elo-talks-ci-$example"
profile_file="$(mktemp)"
loaded=false
cleanup() {
  if [[ "$loaded" == true ]]; then
    sudo apparmor_parser -R "$profile_file"
  fi
  rm -f "$profile_file"
  rm -rf "$stage"
}
trap cleanup EXIT

if [[ "$(cat /proc/sys/kernel/apparmor_restrict_unprivileged_userns 2>/dev/null || true)" == 1 ]]; then
  [[ "${GITHUB_ACTIONS:-}" == true ]] || {
    echo 'Run this CI-only test on GitHub Actions; no host security settings were changed.' >&2
    exit 1
  }
  # Only this exact synthetic test executable gains userns permission. Neither
  # the global kernel restriction nor the Chromium sandbox is disabled.
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

mkdir -p tests/test-results/native
# Exercise GTK fallback dialogs on this isolated display; a real desktop portal
# requires its owning desktop session and is checked during artifact acceptance.
runner=(timeout 30s)
# Debug only synthetic fixtures; production processes and live profiles are never traced.
if [[ "$example" == layout_smoke && "${GITHUB_ACTIONS:-}" == true ]]; then
  runner=(timeout --signal=INT --kill-after=10s 30s gdb --batch --return-child-result
    -ex "set pagination off" -ex run -ex "thread apply all bt 20" --args)
fi
arguments=()
if [[ "$example" == media_permission_smoke ]]; then arguments+=(--use-fake-device-for-media-stream); fi
for scale in 1 2; do
  echo "Running $example at scale $scale"
  xvfb-run -a -s '-screen 0 3200x2000x24' \
    env LD_LIBRARY_PATH="$CEF_PATH${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}" \
    GDK_SCALE="$scale" LIBGL_ALWAYS_SOFTWARE=1 GTK_USE_PORTAL=0 GDK_DEBUG=no-portals GIO_USE_VFS=local \
    dbus-run-session -- "${runner[@]}" "$binary" "${arguments[@]}" 2>&1 | tee "tests/test-results/native/$example-scale-$scale.log" || {
      code=$?
      echo "::error::Native $example failed at scale $scale (exit $code)"
      exit "$code"
    }
done
