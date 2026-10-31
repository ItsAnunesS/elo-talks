# 0.2.0 stabilization

The 0.2 line is a release candidate until manual acceptance is complete. It is not a
claim of certified distribution support or vulnerability-free software.

## Implemented workflows

- Account cards show unread badges, QR sign-in hints and load failures. The overview
  distinguishes accounts preparing in the background from accounts needing attention.
- Account switching is serialized. Opening a dialog waits for an in-flight switch
  before hiding remote views; workspace switching finishes on the requested overview.
- One failed background account does not stop the remaining accounts from starting.
- A notification pause control keeps badges active and preserves per-account settings.
- Updates has its own settings tab with version/channel, current status, a rate-limited
  Check now action, configurable interval and Restart to update.
- Avatar extraction stays memory-only, with at most seven attempts per navigation
  and a new budget on Retry. Initials remain a supported fallback.

## Release policy

`release.json` owns the base version and channel. Every push currently builds
`0.2.0-rc.<github.run_number>` and publishes an immutable GitHub prerelease. It never
replaces the latest stable release. Run numbers intentionally need not start at one.
The checked-in `0.2.0-rc.1` is the development version, overwritten only in CI's build
checkout. No automated version commits are created.

Stable installations stay on stable releases. Installing a candidate is an explicit
manual opt-in; that candidate can update to newer candidates of the same base version
and then to a newer stable release. Candidate queries use one bounded releases-list
request. They retain the same signature verification, cooldowns and closed-vault
installation requirements. External AppImage managers have a pinned candidate tag
so they cannot accidentally replace it with an older stable build; use the built-in
updater for candidate-to-candidate upgrades. Stable builds keep the usual latest source.

Promoting to stable requires changing the channel to `stable` after acceptance.
Published tags/assets cannot be reused for a different commit. Do not promote solely
because CI is green.

## Acceptance gate

| Check | Evidence required |
|---|---|
| Three real accounts | Messages and notifications after 30 minutes in inactive accounts and a hidden window |
| AppImage update | Candidate-to-candidate and candidate-to-stable transition, fresh process and correct version |
| Vault | Screen lock/suspend close all profile users before unmount; failure remains visible |
| Login recovery | Restart, offline startup, unavailable keyring and hardware-key/password fallback |
| Photos | Correct self image or initials; seven-attempt stop and Retry budget reset |
| Clean distributions | Fedora 44 and Ubuntu 24.04 without extra packages or disabled protections |
| Performance | Versioned before/after measurements with the same three accounts and activity; distinguish RSS from unique memory |

Automated unit, browser and isolated native tests provide regression coverage. They
do not substitute for the hardware, clean-system or real-account checks above. See
[VALIDATION.md](VALIDATION.md) for recorded evidence and outstanding tests.
