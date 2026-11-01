# Working on Elo Talks

## Product and communication

Elo Talks is a Linux desktop app for isolated WhatsApp accounts organized into
Discord-style workspaces and channels, with an encrypted local vault. Preserve
its restrained, premium appearance and straightforward workflows.

- Communicate with the maintainer in Portuguese unless asked otherwise.
- Write code, function names, comments, tests, documentation and UI copy in English.
- Apply KISS, DRY and SOLID pragmatically. Prefer small focused changes and existing
  abstractions over speculative frameworks or dependencies.
- Prioritize security, reliable background accounts, performance and accessibility.
- The 0.4 series focuses on security and reliability. Windows/macOS are a future
  portability study; do not claim current support. Do not introduce RC releases
  unless the maintainer changes that preference.

## Repository map

- `src-tauri/src/`: Rust controller, vault, native launcher, GTK/WebKit sessions,
  IPC, notifications and update orchestration.
- `ui/`: local HTML/CSS/JavaScript shell. Remote WhatsApp views are separate.
- `updater/`: Go updater and signature/download/replacement tests.
- `src-tauri/examples/`: isolated native smoke tests and diagnostic tools.
- `tests/`: Python checks and synthetic browser workflows.
- `scripts/`: packaging, versioning, validation and publication.
- `vendor/glib/`: documented security patch; preserve its regression checks.
- `docs/SECURITY.md`: threat model and limitations.
- `docs/VALIDATION.md`: acceptance evidence and outstanding native tests.
- `docs/RELEASE-0.4.md`, `docs/REVIEW-0.4.md`: current work and review findings.

Read the relevant implementation and documentation before changing a boundary.
Inspect `git status` and preserve unrelated work, including diagnostic experiments.
Do not silently include all untracked files in a release commit.

## Security invariants

- Account profiles, cookies, IndexedDB and session caches stay inside the encrypted
  vault. Never extract profiles into a plaintext temporary directory.
- Stop account processes before normal unmount. Never use lazy/forced unmount to
  claim the vault is locked. A successful RPC acknowledgment is not proof of
  closure; retain a recovery state until unmount is confirmed.
- Fail closed when session monitoring is unavailable or the Linux session is
  unsafe. Preserve screen-lock, suspend and inactivity protections.
- Remote content receives no Tauri filesystem, shell or vault permissions. Keep
  local-command origin checks, per-account isolation, TLS validation and WebKit
  sandbox enforcement intact.
- Never disable the host's protections to make a test or packaged app pass.
- Keep PIN secrets in a supported secure keyring; no plaintext fallback. Preserve
  authenticated encryption and master-password recovery when changing auth flows.
- Do not log credentials, QR codes, messages, caller identities, profile-image
  contents or private resource URLs. Diagnostics should contain bounded state and
  counters, not conversation content.
- Notification previews remain opt-in and obey privacy, muting and Do Not Disturb.
- Use bounded inputs, atomic persistence and explicit errors. Never overwrite valid
  state with data that the app cannot subsequently read.
- Do not describe the app or dependencies as vulnerability-free or certified.

## Launcher and updates

- The bundled native launcher opens before the account UI; it must not load
  WhatsApp, access account profiles or ask for vault credentials.
- Reuse the controller's private PID/UID-checked IPC and bundled verifier.
- Verify release identity, signature and checksum before installation. Do not
  weaken verification to recover from network or packaging errors.
- Offer the current version after failed checks or preparation. During actual
  installation, do not permit continuation or voluntary shutdown to interrupt it.
- Only replace the AppImage after confirmed vault closure. Preserve expected-version
  restart verification and recovery from installer-start failures.
- Background checks/downloads must not close active accounts or install without
  the user's restart action. Respect GitHub retry deadlines and backoff.
- Maintain embedded AppImage update metadata and zsync compatibility for Gear Lever
  and other compatible managers.

## UI and performance

- Keep WhatsApp accounts alive when switching channels or workspaces. Do not trade
  background notifications for apparent CPU savings.
- Prefer event-driven updates, bounded retries and one in-flight operation over
  overlapping polling or unbounded DOM scans.
- Reuse cached state and avatar requests appropriately; invalidate on actual
  changes and clear sensitive runtime caches when locking/removing accounts.
- Preserve native account-view geometry when changing local layout.
- Support keyboard operation, visible focus, meaningful accessible names, focus
  restoration, reduced motion and clear error/recovery states.
- Never obscure a security failure behind a generic spinner or a disabled login
  form. Explain the next action without requiring technical knowledge.
- Validate visual edits at normal and minimum supported window sizes. Avoid new
  tests that merely repeat CSS declarations; test meaningful workflow regressions.

## Validation

Run checks appropriate to changed behavior. The release workflow is the canonical
full-check list; use pinned toolchains and lockfiles rather than upgrading tools
incidentally. Common checks from the repository root:

```sh
cargo fmt --all --check
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
npm run format:check
npm run test:ui
python3 -m unittest discover -s tests -v
black --check scripts tests
python3 scripts/check-commit-messages.py
git diff --check
```

Run `go test ./...` from `updater/` for updater changes. Browser tests require
Playwright/Chromium; `PLAYWRIGHT_MODULE` and `CHROME_BIN` can select installed
runtimes. The CI helpers may install narrowly scoped AppArmor profiles on CI only;
do not run those administrative changes on the user's desktop.

Run native smoke tests in an isolated graphical environment with synthetic data:

```sh
cargo build --locked --example layout_smoke --example notifications_smoke --example launcher_smoke
dbus-run-session -- timeout 30s target/debug/examples/notifications_smoke
for scenario in current ready error installing closure; do
  xvfb-run -a dbus-run-session -- timeout 15s target/debug/examples/launcher_smoke "$scenario"
done
```

Use `scripts/test-native-layout.sh` only in its documented compatible environment.
Live-account diagnostics require the user's authorization and a test account. Never
open, terminate or modify the installed app's real sessions just to run a smoke test.
Synthetic tests do not certify GNOME, clean-distribution compatibility, FIDO2,
suspend/resume or a real AppImage upgrade. Report those limits explicitly.

## Git and releases

- Follow the requested branch; the project's publication branch is `release`.
- Use Conventional Commits, e.g. `fix(vault): preserve recovery after unmount failure`.
- Preserve the maintainer's configured Git author/committer identity and signing
  setup. Do not substitute an agent identity or add Co-authored-by trailers. Do not
  claim a cryptographic signature unless verified.
- Commit/push when authorized in the task or conversation. Do not rewrite history,
  force-push or alter published tags without authorization for that operation.
- `release.json` and `scripts/release_version.py` define version numbering. Check
  the current workflow run number before changing the start of a release series.
- CI sets build versions; do not add automated version-bump commits.
- Keep Actions pinned by SHA, least-privilege permissions, immutable releases and
  correct latest-version ordering. Preserve prior push builds rather than cancelling
  them. Do not add Dependabot or unrelated workflows without a new user request.
- Publisher: `AnunesS`. Application identifier: `dev.anuness.elotalks`. Keep Sigstore
  provenance and checksums.
- A push is not a successful release. Report commit/push status separately from CI
  results and installed-AppImage verification.

## Handoff

Describe the behavior changed, relevant validation and remaining limitations.
Do not claim a complete audit, measured speedup or real-system acceptance based
solely on source review and synthetic tests. Keep experimental features separate
from production behavior until their required signals and failure modes are proven.
