# Validation

This document distinguishes completed checks from outstanding acceptance criteria. A successful build does not certify WhatsApp compatibility or package security.

## Automated checks

- Rust: authenticated envelopes, incorrect credentials/context, tampering, password rotation without re-encrypting the vault, atomic writes, locked mutation rejection, and RPC validation.
- Python: synchronized versions, local-only capabilities, publication behavior, sandbox adapter, and the vendored security patch.
- Separate FUSE test: initialization, encrypted content, unmount, incorrect password, and restored content.
- CI: formatting, Clippy, tests, audit, and packaging before publication.

## Required manual acceptance

| Scenario | Clean Fedora 44 | Clean Ubuntu 24.04 |
|---|---|---|
| Run AppImage without installing packages | Pending | Pending |
| Create vault, close, reboot, and unlock | Pending | Pending |
| QR login with two isolated real accounts | Pending | Pending |
| Messages after reboot and offline operation | Pending | Pending |
| Notifications with closed window and account inactive for 30 minutes | Pending | Pending |
| Media, attachments, and explicit export | Pending | Pending |
| Screen lock, suspend, and logout | Pending | Pending |
| PIN with locked/unavailable keyring | Pending | Pending |
| Compatible YubiKey enrollment, unlock, failure, and removal | Pending | Pending |
| Abrupt termination, full disk, and residual mount | Pending | Pending |
| Inspection of writes outside the vault | Pending | Pending |

Do not mark these complete based on simulated interface tests. They require real desktop sessions, hardware, and accounts. Disabling sandboxing or installing dependencies does not satisfy clean-system acceptance.

## FUSE procedure

Build the test binary and bundled helper. Run `vault::tests::fuse_persistence_and_ciphertext` with `--ignored --exact` and `ELO_TEST_HELPER` pointing to the helper. The test uses temporary directories and synthetic data, never the user's vault.

## Recorded evidence (2026-10-29)

- Built in an Ubuntu 24.04 container with Rust 1.99.1 and Tauri 2.12.5.
- Eight Rust and eight Python tests passed; application Clippy checks passed.
- Real FUSE persistence test passed on the developer's Fedora installation: creation, writing, ciphertext marker inspection, incorrect password, reopening, and restoration. It ran as a temporary user service because the command runner enforces NoNewPrivs.
- Chromium frontend tests covered PIN entry, authentication rejection, clearing secrets, safe rendering of malicious-looking account names, and account creation. The backend is mocked; these do not validate WebKit/WhatsApp.
- Rust audit reported no known vulnerabilities after the verifiable local RUSTSEC-2024-0429 fix. Four maintenance warnings remain documented in SECURITY.md.
- AppImage built with WebKitGTK, media libraries, libfido2, and gocryptfs.
- Native AppImage startup confirmed on Fedora with temporary data and no real accounts.
- Upstream helper cryptography tests passed after Go dependency updates; govulncheck found no reachable vulnerabilities.

The developer's Fedora installation is not a clean system. These checks do not replace the acceptance table.

## Sandbox corrections

Initial startup used WebKitGTK 4.1's default, which does not enable sandboxing. Making it mandatory exposed missing helpers and relative paths incompatible with the namespace. The package now contains bwrap, xdg-dbus-proxy, and a path adapter preserving seccomp and namespaces. It exposes system `/usr` libraries and the application bundle read-only.

An interface crash no longer causes a restart loop: processes stop, the vault closes, and the application exits. Subsequent visual tests run in an isolated virtual desktop, not the user's GNOME session.

The native interface was rendered and captured under Xvfb in a container with mandatory WebKit sandboxing and software Mesa rendering. The local screenshot is `dist/native-test/native.png`.

## Integrated account layout

Tauri/Wry's Linux GtkBox container does not apply positional bounds to child webviews. The shell and accounts now share a GtkOverlay: the shell fills the window; accounts have native margins of 304 logical pixels on the left and 72 at the top. GTK manages resizing and scaling. Remote fullscreen is disabled and its signal is intercepted.

`layout_smoke` exercises real Tauri/WebKit composition with two synthetic pages, without accounts or personal data. It passed in isolated Xvfb with mandatory sandboxing at 100% and 200% scales: switching, returning to overview, shrinking to 900×640, growing to 1400×900, account bounds, and fullscreen rejection. CI runs it before packaging. It does not replace real WhatsApp conversation, attachment, or notification testing.

### Ubuntu runner AppArmor restriction

The native GitHub Actions test failed with `bwrap: setting up uid map: Permission denied`. Ubuntu's user namespace restriction prevents the unprofiled fixture from starting its sandbox. CI now installs a temporary profile granting `userns` only to the exact `layout_smoke` executable. The script removes it on exit, including failure. No global sysctl or WebKit sandbox is disabled. The profile is not packaged in the AppImage.

**This validates layout under a CI-specific permission, not default Ubuntu 24.04 compatibility.** Running without administrative configuration remains pending and requires validation or architecture changes before claiming certified support. See [Ubuntu user namespace restrictions](https://documentation.ubuntu.com/release-notes/24.04/#unprivileged-user-namespace-restrictions).

## Native notification regression

The synthetic WebKit layout test initializes notification permission for the WhatsApp
origin before loading, receives a real `show-notification` event, and receives another
from a timer while both the account and its window are hidden. It then restores the
window and checks the account layout. This does not establish 30-minute real-account
connectivity; that manual acceptance remains pending.

`notifications_smoke` runs on an isolated D-Bus session with a fake desktop notification
service. It checks fixed private payloads, more than four undismissed deliveries,
per-account replacement, click routing, explicit test notifications, safe errors and
recovery after the notification service loses and reacquires its bus name. It does not
claim that a real compositor displayed a banner. The in-app Linux notification test
lets the user verify that final step independently of WhatsApp settings.

A generic direct D-Bus notification was accepted by GNOME on the developer's session,
and the user confirmed seeing its banner. This verifies desktop display, not message
receipt from a logged-in WhatsApp account in the updated build.

The pinned ZapZap source comparison, release hashes, user-reported v0.1.17 failure
and real-account test matrix are recorded in
[WhatsApp integration evaluation](WHATSAPP_INTEGRATION.md). ZapZap remains a source
reference only, as requested. The native regression reproduced a missing
Permissions API response (`prompt` despite `Notification.permission: granted`).
It now checks the correction alongside origin and feature restrictions. A real
delivery observation is recorded below; long-idle acceptance remains pending.

On 2026-10-30 the user confirmed that a real message notification appeared, with
the generic message text. This establishes that delivery worked in that reported
case; it does not establish all background durations or clean-system acceptance.

Notification-content regression coverage checks private defaults for existing
preferences, preference persistence at the vault path, exclusion of channel labels from
diagnostics, bounded Unicode previews, markup escaping and missing-content
fallbacks. The isolated D-Bus fixture checks the exact private/channel/preview
payloads and unchanged account click routing. UI coverage verifies explicit
preview selection, saving and restoring the choice. Real sender/group formatting
still depends on what WhatsApp includes in its native notification.

## Automatic updates

Synthetic tests cover strict release/version selection, checksum binding, unsigned/tampered rejection, symlink rejection, atomic replacement with a preserved previous executable, background checks without installation, startup installation sequencing, and offline unlock fallback. Publication also validates its newly issued Sigstore bundle using the helper extracted from the built AppImage.

Still requires desktop acceptance: updating from one published updater-enabled AppImage to the next, restart with the vault locked, Gear Lever integration preserving the same launcher path, a read-only installation directory, interrupted download, and unmount failure. Page warm-up is bounded and does not inspect authentication state or messages.

## WhatsApp unread integration

Unit tests cover exact-origin, bounded title parsing; unread count decreases; fallback after failure; and preservation when selecting an account. The native sandboxed fixture updates its title and emits a notification while both its view and window are hidden, then checks acknowledgement and count clearing. The isolated D-Bus fixture delays a reply and verifies that a burst delivers its latest preview instead of dropping it. UI tests distinguish zero reported by WhatsApp from unavailable metadata and aggregate mixed sources across workspaces.

Real WhatsApp acceptance remains necessary: receive messages in an inactive account, compare its badge against the WhatsApp browser-title counter, read chats on another device, and confirm both counter decreases and notifications with the window closed to the tray. The title is an observed UI convention, not an official stable WhatsApp API, and may count chats rather than individual messages.

## Unified startup

The encrypted catalog persists the last selected account and falls back to an existing account after deletion. Locked status does not expose this preference. The primary view starts first; other views start at 500 ms intervals and do not gate the primary view. Structural readiness is tested with a delayed synthetic chat-list element after document load, including a hidden native WebKit view. UI tests cover restoring a non-first account, waiting after page load, QR readiness, background loading without blocking, and explicit Continue anyway. The detector never reads QR contents or messages.

Real WhatsApp markup and network behavior still need acceptance on the supported desktops. If its structural selectors change, the app preserves access through Continue anyway rather than inferring authentication or weakening origin/sandbox policy.


## Account card avatar regression

Synthetic coverage includes PNG normalization and input limits, profile host and
blob-origin rejection, self-control selection with a contact-position decoy,
frontend photo/initials fallback, and exclusion from diagnostics. The isolated native
layout fixture exercises the production blob adapter and a separate cross-origin
image resource: JavaScript canvas export must fail while native WebKit resource
reuse succeeds. Both desktop scale factors run with the WebKit sandbox enabled.

Live WhatsApp acceptance remains manual: after updating and unlocking, verify the
own-user photo on each account card, including background accounts. Check initials
for missing or unsupported photos, logout and return to Overview, and vault lock.
The bounded adapter can miss a first QR login that takes longer than 40 seconds;
reload the account after login to retry without signing out. Do not interpret a
successful synthetic test as proof of compatibility with every WhatsApp rollout.
