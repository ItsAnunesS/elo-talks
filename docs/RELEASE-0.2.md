# 0.2.0 stabilization

The 0.2 line is published directly to the stable update channel. This is not a
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

`release.json` owns the base version and its first GitHub run number. Run 32 builds
`0.2.0`; subsequent pushes build `0.2.1`, `0.2.2`, and so on. Reruns keep the same
version. No automated version commits are created and published assets stay immutable.

Every successful build publishes a regular release. The highest stable version is
marked latest and offered by the built-in updater and compatible AppImage managers.
Signature verification, cooldowns and closed-vault installation remain required.
The updater retains compatibility with previously installed candidate builds so they
can move to the stable release.

## Validation checklist

| Check | Evidence required |
|---|---|
| Three real accounts | Messages and notifications after 30 minutes in inactive accounts and a hidden window |
| AppImage update | Upgrade from the previous installed version, fresh process and correct version |
| Vault | Screen lock/suspend close all profile users before unmount; failure remains visible |
| Login recovery | Restart, offline startup, unavailable keyring and hardware-key/password fallback |
| Photos | Correct self image or initials; seven-attempt stop and Retry budget reset |
| Clean distributions | Fedora 44 and Ubuntu 24.04 without extra packages or disabled protections |
| Performance | Versioned before/after measurements with the same three accounts and activity; distinguish RSS from unique memory |

Automated unit, browser and isolated native tests provide regression coverage. They
do not substitute for the hardware, clean-system or real-account checks above. See
[VALIDATION.md](VALIDATION.md) for recorded evidence and outstanding tests.
