# Validation

This document distinguishes completed checks from outstanding acceptance criteria. A successful build does not certify WhatsApp compatibility or package security.

## Native account presentation - 2026-11-28

Linux browser views are raised above sibling views when shown, instead of merely
remapping them in their previous stacking position. Windowed browsers use native
visibility/configure events and screen-information notification; CEF's
windowless-only `WasHidden`/`WasResized` calls remain scoped to offscreen views.
No account document is reloaded or stopped when switching channels.

The native layout fixture cycles linked and QR-only synthetic documents after
raising the local shell. A single solid-color pixel on the isolated Xvfb display
must match the selected document, and per-document tokens must survive switching.
This verifies actual window presentation rather than a renderer-only screenshot.
Real GNOME/compositor acceptance and the CEF fixture's execution remain pending.

## Same-document WhatsApp Business sign-in - 2026-11-28

The previous startup probe stopped after detecting a QR code. Session linking
that replaced the login UI without navigating could therefore leave the channel
label at "Scan QR code". Structural login-state observation now remains active
in the admitted main document's isolated world and reports QR/chat transitions.
Only the readiness enum crosses the native bridge; conversation text and QR
pixels are not read. Changes inside a stable chat list do not generate duplicate
status reports. TLS and process failures retain precedence.

Browser regressions cover QR-to-chats-to-QR in one document, including a
Business-style navigation layout. Native layout regressions exercise the same
transition without a generation change. Real Business-account acceptance remains
pending; the installed app and its profiles were not accessed.

## File-manager attachments - 2026-11-28

The attachment adapter handles native Ctrl+V file lists and native window drops
for the selected WhatsApp account. Unit checks cover bounded local URI lists,
invalid/remote URLs, folders, special/private paths, symlinks and size limits.
Browser checks use the production capture/dispatch scripts and real Chromium
File objects for PNG/ZIP delivery, focus changes, stale document rejection and
absence of automatic send actions.

`clipboard_smoke` now includes copied file lists, a GTK drag source on the isolated
X11 display, and hidden-account isolation. The fixture compiles locally; native
execution at both scales and real GNOME/WhatsApp attachment-preview acceptance
remain pending. The local container's Chromium sandbox limitation is unchanged.
Installed profiles and conversations were not used for validation.

## Navigation pointer reordering - 2026-11-27

Local channel and workspace reordering uses pointer capture instead of HTML5
native drag events. Browser regressions exercise real mouse movement, insertion
markers, Escape cancellation, normal clicks, keyboard reordering, managed-item
restrictions, failed saves and cross-workspace moves with Undo.

`channel_drag_smoke` exercises the same gesture handler with real X11 mouse input
and offline synthetic rows in CEF. Checks runs it at 1x and 2x. Local execution
remains pending because the Docker environment could not initialize Chromium's
required sandbox. No host policy or installed-app session was changed. Browser
success does not establish native desktop acceptance.

The first CI run of this fixture (`46041297340`, source `7ae16bf`) timed out
at 1x after reporting pointer events. Other native CEF fixtures passed. The input
runner now bounds each xdotool invocation and avoids synchronous focus/movement
waits. Stage and move/error counters distinguish input hangs from failed reorder
assertions; native acceptance remains pending a successful run at both scales.

Run `46050445597` (`f6ad91b`) passed native channel and workspace reordering at
1x. At 2x it received movement/release events but no initial press, so no reorder
was attempted. The fixture now calibrates X11 coordinates using observed DOM
pointer positions instead of assuming that `devicePixelRatio` is the embedding
transform. Verification at both scales remains pending.

## Full-app CEF candidate — 2026-11-08, unpublished

Completed locally with pinned Rust/Go tools and Ubuntu 24.04 Docker builds,
using isolated Xvfb displays and synthetic data:

- Chromium 156.0.8078.88 / CEF 156.0.26, bindings 156.4.0. No Electron.
- 70 ordinary Rust tests passed, including invalid-header and media-policy regressions.
  Four hardware/host integrations remain ignored in the ordinary suite and are
  reported separately. All-target Clippy passed with warnings denied.
- Host FUSE encryption/persistence and backup restoration passed. Restoration
  preserves catalog/account identifiers, legacy profiles and Chromium profiles.
- The production CEF shell/controller/FUSE test passed: browser shutdown,
  confirmed normal unmount and encrypted backup export.
- Native layout, isolated profiles, readiness, blob/cross-origin own avatars,
  hidden-account notifications, fullscreen rejection and remote capability denial
  passed at 1x and 2x. Clipboard text, valid/invalid image decoding and hidden-view
  isolation passed at both scales.
- Document, registration and service-worker notification transport passed at both
  scales. Native D-Bus privacy, grouping, account clicks and service restart passed.
- Production shell/native IPC, memory protection, remote command rejection and
  opening/canceling both backup dialogs passed at both scales. A regression also
  creates two accounts through the real command and checks completed loads, title
  badges and shell responsiveness. These isolated tests
  exercise GTK's fallback chooser; a live GNOME portal requires its desktop session.
- All seven launcher scenarios passed: current, ready, error, installing, closure,
  cooldown and close. Visual checks covered the shell/backup controls at normal and
  minimum sizes and the GTK4 launcher.
- Browser workflows, formatting and 18 Python regressions passed. Updater tests,
  including the existing network/signature gate, passed; the pinned gocryptfs
  build/crypto tests and reachable-code vulnerability check passed.
- Rust dependency audit passed without new exceptions. Four unmaintained-library
  warnings remain visible. The old build-only rand dependency was replaced through
  a narrow vendored generator manifest change.
- Ubuntu 24.04 AppImage packaging and embedded update-source/zsync verification
  passed for the 0.5.15 candidate. Packaged-shell startup/IPC/normal shutdown also
  passed after correcting the resource paths beside libcef. CI now repeats this
  packaged production-shell gate.

The first live full-app attempt stalled before QR display. A synthetic regression
reproduced a synchronous URL getter inside CEF title callbacks. The callbacks now
use the browser-reported committed origin instead; the regression passes at both
scales. The user subsequently reached a linked account and Chromium's microphone
permission prompt. That establishes startup progress, not successful calls.

Elo-owned native media consent now passed synthetic getUserMedia tests at both
scales with fake devices: allow, deny, per-device/account isolation, revoke,
ask again, and rejection of stale/cross-origin requests. The production shell
rejects remote access to both permission commands. Browser settings tests cover
revocation, reset, and persistence errors; all 70 ordinary Rust tests and Clippy
passed. The user confirmed voice/video calls, Elo consent and permission
revocation/re-prompt in the full 0.5.15 AppImage on 2026-11-08. A follow-up
confirmed incoming calls and login persistence after locking/unlocking the vault.

Remote Checks for eed4eb1 and 946012b failed in the synthetic native layout test:
one scale-2 SIGSEGV and one scale-1 timeout. Three additional local scale-2 runs
passed. Restricting the synthetic fixture to two CPUs under GDB reproduced
the shutdown crash twice: `CefShutdown` destroyed Chromium global services before
a pending `Browser` destructor used `TabDragServiceImpl`/`DropTargetRegistryImpl`.
The runtime now posts a CEF UI-task barrier after all browser close acknowledgments
before exiting its native loop. The reproducer then exited normally three times.
Stage markers and a CI-only debugger capture future failures without skipping gates.
Canonical Checks must still confirm this fix.

The subsequent release check reached `shell_smoke` but timed out at scale 2
while waiting for account readiness and title counters. On 2026-11-25, pinning
the synthetic fixture to one CPU reproduced an account navigation cancelled by
the runtime's delayed initial `about:blank` load. The runtime now replaces its
pending initial destination when the application requests navigation, retaining
the document-start registration barrier. The original source failed twice in
this environment; the fix passed twenty runs, ten at each scale. The fixture
also requires exactly two accounts and reports bounded, content-free health
changes while waiting. No live account or installed vault was accessed.

Remote run `45670146827` then passed all native CEF regressions, including
`shell_smoke` at both scales. Its subsequent dependency audit found the newly
reported `cookie` and `url` advisories. The lockfile now uses their patched
versions, 0.18.3 and 2.5.9, plus the required `form_urlencoded` 1.2.3. The local
audit passes without new exceptions; five unmaintained dependency warnings
remain visible.

Remote run `45675415750` captured a separate layout startup deadlock. Its UI
thread was waiting for the shared script map in a load callback while the
adapter-registration thread held that mutex and waited for a CEF command.
Script registration now completes before locking the map for insertion. The
layout fixture prepares both offline responders before starting the adapters,
checks that exactly two accounts reach completed loads and chat readiness, and
identifies each registration stage in its diagnostics. With the fix, twenty
layout runs passed with one-CPU affinity, ten at each scale; the shell fixture
also passed at both scales. Rust tests, Clippy and the dependency audit passed.

Canonical run `45679771137` passed all checks, AppImage packaging, signature
verification and publication for v0.5.19. Authorized installed-image validation
then confirmed a signed v0.5.14 to v0.5.19 upgrade: normal controller shutdown,
confirmed vault unmount, installation through the official updater, preservation
of the previous image, and startup of the normal UI with the expected-version
guard. The installed checksum matched the signed release. Unlocking and checking
existing accounts remains manual; no installed account content was inspected.
GNOME portal, hardware-key, suspend/resume and clean-distribution acceptance
remain pending.

## Desktop identity and icons (2026-11-25)

Native windows, existing AppImage-manager launchers and the packaged desktop file
now share `dev.anuness.elotalks`. GTK icon resources are embedded at 32, 128 and
512 pixels, so the launcher does not depend on a separately installed icon theme.
The CEF runtime now forwards Tauri's GTK application ID instead of discarding it.
Manager launcher names, commands and unrelated fields remain preserved.

Local Ubuntu 24.04 Docker validation passed 71 Rust tests (four explicit host
integrations remain ignored), Clippy, Python checks, and native launcher/shell
fixtures. A new isolated X11 regression checked the actual `WM_CLASS` and
`_NET_WM_ICON` properties at both scales. The AppImage passed icon inventory,
native IPC startup and normal shutdown checks. Windows ICO and macOS ICNS files
were generated from the existing logo and decoded successfully; no native
Windows/macOS build or desktop acceptance is claimed. GNOME, KDE Plasma and Xfce
integration on real desktops still requires acceptance of this new icon build.

## Automated checks

- Rust: authenticated envelopes, incorrect credentials/context, tampering, password rotation without re-encrypting the vault, atomic writes, locked mutation rejection, and RPC validation.
- Python: synchronized versions, local-only capabilities, publication behavior, sandbox adapter, and the vendored security patch.
- Separate FUSE test: initialization, encrypted content, unmount, incorrect password, and restored content.
- CI: formatting, Clippy, tests, audit, and packaging before publication.

## Launcher and update discovery (2026-11-02)

- Reproduced GitHub REST `403` with an exhausted anonymous rate limit. Stable
  checks now resolve the official public `/releases/latest` redirect with HEAD,
  validate the repository/stable tag and construct canonical asset URLs. RC
  discovery retains its existing opt-in REST channel. No GitHub token is needed.
- Image size remains mandatory and bounded, using asset HEAD metadata when REST
  metadata is absent. Staging/install still authenticate the checksum signature,
  enforce the expected GitHub workflow identity and verify the image digest.
- The built helper discovered the live stable 0.4.8 release, reported it current
  when already installed, then downloaded and authenticated it into a synthetic
  staging directory. The synthetic installed file remained byte-for-byte intact;
  installation/restart of the real desktop app was not attempted.
- Full Go tests passed with real Sigstore TUF access, including modified-manifest
  rejection, public discovery, wrong origins/tags, HTTP failures and image bounds.
- Rust suite: 63 passed, three ignored; Clippy passed with unchanged vendored GLib
  warnings. Safe error categories retain server retry deadlines without displaying
  upstream response bodies/URLs. The launcher shows the reason and retry countdown.
- Real GTK/Xvfb launcher scenarios passed for current, ready, retry, installing,
  closure recovery, cooldown and custom close. The undecorated window has an
  accessible integrated close button, guarded during installation, and a draggable
  brand area. Screens at 500×400 and 460×380 were visually inspected.
- Native CI stages are reported separately, with fixture/scale annotations on
  failure, so a timeout identifies the exact fixture rather than one combined step.

Installed AppImage upgrades and GNOME/Wayland drag/close acceptance remain manual.
These tests used public release data and isolated synthetic windows only.

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

## Server metadata integration (2026-11-02)

- Normal Rust suite: 62 passed, three ignored fixtures. New cases cover metadata
  import/idempotence, local preservation, domain/user/org/workspace profile
  namespaces, invalid snapshots, status token exclusion and locked commands.
- The ignored HTTPS/PostgreSQL client fixture was executed explicitly and passed
  against a real disposable service with a verified certificate and scoped member
  access. It did not mount a real vault or open WhatsApp.
- Complete Playwright UI regressions passed with the installed Chrome and sandbox
  enabled, including domain sign-in, cleared password, organization selection and
  a minimum-size settings capture, alongside the existing backup/notification/UI
  workflows. Clippy passed with the existing vendored GLib warnings unchanged.
- The backend's PostgreSQL API suite and real-browser panel workflow separately
  verified roles, cross-organization denial, grants and revocation. They do not
  certify native account process closure or real-system performance.

See SERVER-SYNC.md for the supported one-way metadata scope, lease/recovery
behavior, desktop bounds and remaining real-account/AppImage acceptance checks.
No changes were made to the clipboard adapter or existing release-series offset.

## Encrypted backup validation (2026-11-02)

- Rust suite: 59 passed, two ignored (host-FUSE fixtures). Snapshot tests cover
  password separation, preserved multi-record profile data and empty files,
  independent restored vault identity, PIN removal, wrong-password rejection,
  authenticated tampering/truncation/trailing-data rejection, record reordering,
  bounded lengths, unsafe paths, links, cancellation and no-clobber publication.
- Controller tests reject export/import while unlocked or in closure recovery,
  serialize backup work against create/unlock and discard canceled restore staging.
- `cargo clippy --locked --all-targets -- -D warnings` passed with the pinned
  Rust 1.99.1 toolchain in the existing isolated build container. Existing vendored
  GLib compiler warnings remain; no vendor source was changed. For compile/test
  only, `TAURI_CONFIG` omitted external packaging binaries unavailable locally;
  production configuration retains all bundled helpers.
- The complete synthetic Playwright interface suite passed with the installed
  Chrome and its sandbox enabled, using a fresh temporary profile. It includes
  password confirmation errors, locked-only backup actions, import on a new
  installation, busy-state unlock prevention, completion feedback and cleared
  dialog secrets. Export/import screens were inspected at 900×640, with an
  additional export capture at 1200×800. Chromium could not start inside the
  nested Docker sandbox; host protections were not altered.
- Fourteen Python tests, frontend formatting, JavaScript syntax, commit-subject
  conventions and whitespace checks passed.

The new `backup::tests::fuse_backup_restores_readable_catalog_and_profile_data`
fixture requires host FUSE and `ELO_TEST_HELPER`, like the existing vault fixture.
It remains ignored in ordinary runs. Actual migration of WebKit/WhatsApp profiles
between two Linux PCs, no-QR login preservation, native file chooser interaction,
removable-filesystem behavior, full-disk/power-loss behavior and a packaged 0.5
AppImage upgrade remain manual acceptance checks. No live account was accessed,
and no commit, push or new release publication is implied by these results.

## Clipboard regression (2026-11-02)

`clipboard_smoke` uses an ephemeral WebKit context and a synthetic PNG under an
isolated Xvfb/D-Bus session. It covers Ctrl+V image delivery without duplicate
events, Shift+Insert text fallback, invalid PNG rejection, cancellation when the
account is hidden during a clipboard request, and exclusion of other origins.
The fixture requires `xdotool` and sends actual X11 shortcuts from a separate
process to its own PID's window. The deprecated synchronous GTK key helper
stalled during image paste on the Ubuntu runner. A separate watchdog reports
the last stage instead of leaving an unexplained outer timeout. All original
paste, text, cancellation and origin assertions remain required.

The clipboard adapter now registers a top-frame no-op user script restricted to
the WhatsApp origin. This retains the named isolated JavaScript world between
target capture and image delivery: WebKit's shared named-world map uses weak
references, so two standalone evaluations can otherwise get different worlds
and silently lose `eloPaste`. No page bridge or clipboard data is injected by the
registration itself. Focus, origin, generation and target-identity guards remain.
Modifier-only key events no longer cancel an in-flight paste. WebKitGTK can
asynchronously propagate an unhandled modifier back through GTK after the paste
shortcut; the previous unconditional cancellation made image delivery depend on
event ordering. Native reproduction also showed Control/Shift arriving with
`GdkEventKey.is_modifier()` false, so the adapter classifies their key symbols.
Editing/navigation keys, pointer events, focus loss, hiding and navigation still
cancel, and delivery still verifies the original target. The
fixture now queues a short synthetic web-process task and sends an extra Shift
key after each shortcut, exercising a modifier while target capture is pending.

Run `bash scripts/test-native-layout.sh clipboard_smoke` in the same documented
compatible environment as the layout fixture. `--native` bypasses the adapter
for comparison with the engine's own paste behavior.

The example compiles with the pinned toolchain. Native execution used a disposable
non-root container with private Xvfb/D-Bus, no network or host mounts and WebKit's
sandbox enabled. Namespace and proc allowances were scoped to that test container;
no host protection settings changed. The original implementation, rebuilt against
the strengthened fixture, failed image paste as expected; the key-symbol fix passed
the complete fixture twenty times (ten runs each at GDK scales 1 and 2). The
Rust suite passed 63 tests (three explicit host/integration
fixtures remain ignored), and Clippy passed. Actual WhatsApp attachment preview and sending
from a screenshot, including GNOME/Wayland clipboard behavior, remain manual
acceptance checks; no real account was accessed for this change.

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
preview selection, saving and restoring the choice. Sender-photo coverage checks
bounded PNG decoding, the D-Bus image-data signature and privacy downgrades.
Real sender photos and desktop rendering still require WhatsApp/GNOME acceptance.
Real sender/group formatting
still depends on what WhatsApp includes in its native notification.

## Automatic updates

Synthetic tests cover strict release/version selection, checksum binding, unsigned/tampered rejection, symlink rejection, atomic replacement with a preserved previous executable, background checks without installation, startup installation sequencing, and offline unlock fallback. Publication also validates its newly issued Sigstore bundle using the helper extracted from the built AppImage.

Still requires desktop acceptance: updating from one published updater-enabled AppImage to the next, restart with the vault locked, Gear Lever integration preserving the same launcher path, a read-only installation directory, interrupted download, and unmount failure. Page warm-up is bounded and does not inspect authentication state or messages.

## WhatsApp unread integration

Unit tests cover exact-origin, bounded title parsing; unread count decreases; fallback after failure; and preservation when selecting an account. The native sandboxed fixture updates its title and emits a notification while both its view and window are hidden, then checks acknowledgement and count clearing. The isolated D-Bus fixture delays a reply and verifies that a burst delivers its latest preview instead of dropping it. UI tests distinguish zero reported by WhatsApp from unavailable metadata and aggregate mixed sources across workspaces.

Real WhatsApp acceptance remains necessary: receive messages in an inactive account, compare its badge against the WhatsApp browser-title counter, read chats on another device, and confirm both counter decreases and notifications with the window closed to the tray. The title is an observed UI convention, not an official stable WhatsApp API, and may count chats rather than individual messages.

## Unified startup

The encrypted catalog persists the last selected account and falls back to an existing account after deletion. Locked status does not expose this preference. The primary view starts first; other views start at 500 ms intervals and do not gate the primary view. Structural readiness is tested with a delayed synthetic chat-list element after document load, including hidden native CEF views and a delayed synthetic QR container with a centered logo. UI tests cover restoring a non-first account, waiting after page load, QR readiness, background loading without blocking, explicit Continue anyway, automatic access after the 40-second probe deadline, and retaining manual recovery for load errors. The readiness probe starts when the trusted main document commits, without waiting for all resources to finish. The native QR fixture leaves an intercepted image request pending and requires QR readiness while page state remains loading; the UI fixture checks that this state releases the startup overlay. Failed page states cannot acquire readiness from a pending query. The detector never reads QR contents or messages.

Local validation on 2026-11-26 passed 71 Rust tests (four host-dependent tests ignored), Clippy for all targets, Rust/Prettier formatting, 18 Python tests and the browser suite, including QR readiness before page load completion. The updated native fixture compiled, but its Docker run aborted during Chromium sandbox initialization before exercising the scenario. Native acceptance remains pending; host protections were not changed.

Real WhatsApp markup and network behavior still need acceptance on the supported desktops. If its structural selectors change, the app removes its loading overlay after 40 seconds so the channel remains accessible. Load errors still require explicit recovery. Neither fallback infers authentication or weakens origin/sandbox policy.


## Session persistence regression

The browser policy is shared by the production UI and `session_persistence_smoke`.
It enables session-cookie persistence on every independent account context while
keeping the bootstrap context memory-only. The fixture stores session cookies,
persistent cookies, localStorage and an IndexedDB record containing a
non-extractable CryptoKey. It restarts the browser process using the
controller's shutdown helper, including a forced-stop recovery, migrates a closed
populated profile from the old nested directory, checks the stored key can still decrypt its synthetic record,
and checks another account has none of this state. It also rejects unexpected
restored browser targets. All requests are
fulfilled offline, and DNS resolution is blocked in this fixture to prevent
unhandled restored targets from contacting external sites. The fixture explicitly
uses a mock cookie key in its temporary synthetic profile; it does not exercise a real system keyring or WhatsApp account.

CI reproduced loss of all four storage types across a clean restart with the
previous nested profile layout. Chrome-backed CEF accepts only direct children of
its root cache and otherwise falls back to an off-the-record profile, as documented
in [CEF issue 3930](https://github.com/chromiumembedded/cef/issues/3930). The corrected
account layout preserves the root cache and renames account directories atomically
without overwriting a destination. The runtime's restore-on-startup preference now
follows CEF's session-cookie persistence policy, instead of replacing it with NewTab and discarding session
cookies. The native fixture rejects any additional browser targets after reopening;
new-window requests remain denied.

Migration runs before CEF initialization so ProfileManager cannot recreate an
empty destination from its previous profile metadata before migration finishes.

Local Docker validation on 2026-11-26 passed 76 Rust tests (four host-dependent
tests ignored), Clippy for all targets, 18 Python tests, browser workflows and
Rust/Python/Prettier formatting. The process tests cover a session write taking
more than one second and a forced stop reported as unsuccessful. The native
persistence fixture compiled, but Docker denied Chromium's namespace creation
with its required sandbox. No sandbox or host restriction was disabled. The
Checks workflow now requires this fixture at both desktop scale factors before
packaging; its result and real WhatsApp acceptance remain separate evidence.

## Account card avatar regression

Synthetic coverage includes PNG normalization and input limits, resource scheme and
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


Interactive avatar diagnosis uses `cargo run --locked --example avatar_probe --
/path/to/private/report.json [optional-adapter-script]` on a user-authorized test
desktop. Use a test account: this ephemeral WebKit session is lost on close. The
report includes only structural flags, image format/size and conversion status,
never source URLs, photo pixels, QR contents or conversation text. The small preview
exists only in memory. It is a diagnostic example, not part of the AppImage runtime.

A live test account reproduced the rejected-host problem: the self control and
its loaded JPEG resource were found, but the two-host filter rejected the source.
Native reading and 64×64 conversion succeeded while canvas remained unavailable.
The fix accepts eligible HTTPS resources only when they exactly match the selected
self control; it does not fetch URLs. Final overview-card confirmation in the
packaged release remains a separate acceptance check.

Safe diagnostics includes `avatar`, `avatarAttempts` and `avatarProbeActive` for
each account, without identifying the account or exposing image URLs/pixels.
Retry reloads the WhatsApp page, clears the previous photo and starts a new budget
of at most seven attempts. Search stops earlier on success.

## 0.2 candidate preparation (2026-10-31)

Local automated validation passed for the candidate: 42 Rust tests (the separately
run FUSE test remains ignored in the ordinary suite), 14 Python contracts, Go
updater tests including a real Sigstore fixture, browser workflow tests, native
D-Bus notification tests, and the native WebKit layout suite at both scale factors.
Clippy, formatting and dependency audits completed. Go reported no reachable
vulnerability; three findings in required modules were outside called/imported
code. Rust retains four documented maintenance warnings.

Browser coverage includes notification pause without clearing badges, update
interval persistence through the controller tests, a manual-check cooldown, stable
vs candidate release ordering, candidate publication without changing stable
latest, and continuing background startup after one account fails. Browser tests
use synthetic data; they do not prove live WhatsApp health.

The attempted process-counter sample found no running Elo Talks process. It is
not a usable CPU/memory baseline and no performance improvement percentage is
claimed. The three-account soak, clean distributions, hardware credentials and
installed-AppImage candidate upgrade acceptance remain pending.

## Voice and video permission acceptance

The account adapter explicitly enables WebRTC/media streams and prompts for native
camera/microphone requests. Unit coverage checks that media permission queries
require consent and unrelated permissions/untrusted origins remain denied.

Local validation (2026-11-08): the pinned Rust 1.99.1 toolchain in Docker with
WebKitGTK 2.50.6 passed 64 Rust tests (3 existing ignored tests), including the new
permission regression. The 18 Python tests also passed. The test-only TAURI_CONFIG
override omitted unavailable packaging helper binaries; this was not an AppImage
build or a live call test.

Pending manual acceptance in an isolated desktop and then an authorized test account:
- Confirm Allow/Deny, window close, navigation and account switching resolve requests.
- Confirm microphone-only and camera+microphone capture with synthetic devices.
- Verify both outgoing and incoming WhatsApp voice/video calls with the packaged
  AppImage, including device denial, missing devices and call termination.
- Verify packaged WebKit/GStreamer supports the required WebRTC codecs; enabling
  settings alone does not establish compatibility or WhatsApp feature availability.
