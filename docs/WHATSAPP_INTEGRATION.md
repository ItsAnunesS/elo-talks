# WhatsApp integration evaluation

## Full application migration — candidate, not yet published

The application source now uses Tauri 3/CEF, GTK4 launcher/file choosers and
separate encrypted Chromium contexts, without Electron. The native notification
adapter blocks direct Chromium delivery and passes synthetic document,
registration and service-worker transport tests. Existing D-Bus privacy/click
regressions also pass. Isolated-world readiness/avatar queries, account storage,
hidden notification delivery, layout, fullscreen denial, clipboard and real
CEF-process shutdown before encrypted backup have native regression coverage.

The full AppImage, real calls in that artifact, desktop acceptance and publication
are still pending. The prototype findings below are historical and do not by
themselves validate the migrated release. The WebKit-only `avatar_probe` has been
retired; use the CEF synthetic avatar regression and private in-app diagnostics.


## Full-app integration release gate — 2026-11-08

Work is isolated on `codex/tauri3-cef-integration`. The maintainer authorized
new Chromium profiles inside the existing encrypted vault and relinking each
account by QR, while retaining existing WebKit profiles for recovery. No session
database conversion or deletion is authorized by that migration strategy.

**Full-app integration and publication are not complete.** The calls prototype
is not evidence that the existing app's behavior has been preserved. Production
dependencies and `release.json` remain unchanged; no new release was published.

Before changing production notification delivery, `notification_test.py` tested
the actual CEF build against a synthetic `org.freedesktop.Notifications` service
on an isolated D-Bus session and Xvfb display. Chromium delivered one native
notification successfully, including its synthetic private sender and body.
The required Elo private-mode contract (generic summary/body, no preview)
failed, with exit code 1 and `privateModePassed: false`. This demonstrates that
directly allowing Chromium's native notifications cannot replace the existing
adapter while preserving Elo's default privacy behavior. It does not indicate a
regression in the installed, unchanged WebKit application.

The inspected `tauri-runtime-cef` alpha.7 source exposes notification permission
handling but no equivalent of WebKit's `show-notification` content/delivery hook.
Notification presentation must be adapted and verified, including service
workers, rather than assuming the Tauri notification plugin intercepts remote
browser notifications. Native Chromium delivery must not bypass Elo's private,
channel and preview modes, per-account mute/focus policy, or click routing.

Reproduce the blocking gate with the built probe and an Xvfb executable:

```sh
ELO_SYNTHETIC_DBUS=1 dbus-run-session -- python3 prototypes/tauri-cef/notification_test.py \
  /absolute/path/to/elo-cef-probe /absolute/path/to/Xvfb
```

The test refuses to replace an existing notification service, uses only fixed
synthetic notification content, and prints booleans rather than page content.
Its current failure must not be suppressed or treated as a passing release gate.
Other full-app gates remain outstanding: the GTK3 launcher/GTK4 process boundary,
multi-account layout/isolation and background delivery, avatars, clipboard and
downloads, complete child-process shutdown before vault unmount, all existing
authentication/backup/sync regressions, and signed AppImage update acceptance.

## Tauri 3 / CEF prototype — 2026-11-08

Branch: `codex/tauri3-cef-prototype`. The separate workspace at
`prototypes/tauri-cef` pins Tauri 3.0.0-alpha.6, runtime-cef 3.0.0-alpha.7,
tauri-build 3.0.0-alpha.4 and CEF 154.5.0 (Chromium 154.0.8037.98).
There is no Electron dependency. Production source, vault format and runtime
dependencies remain unchanged; this is not a migrated or released Elo client.

Completed validation:

- Docker release build with Rust 1.99.1 and Debian Trixie / GTK4 succeeded.
  Bookworm's GTK 4.8 was insufficient (the runtime needs GTK >= 4.14).
- Rust formatting and release Clippy with warnings denied passed.
- The official CEF minimal archive was downloaded outside Docker after a DNS
  failure in the container. Its published size and SHA-1 matched the CDN index.
- A non-root Docker execution aborted because its sandbox could not initialize.
  No `--no-sandbox`, privileged container, or relaxed host policy was used.
- The same executable ran with `SandboxPolicy::Required` in isolated host Xvfb.
  It denied capture from virtual devices and successfully transmitted generated
  audio and video between two local peers: audio bytes arrived and video frames
  decoded. Result code 0. Xvfb emitted GPU/GLX warnings; this is not evidence of
  hardware-accelerated rendering or production performance.
- The live route rejected an unsupervised launch and an unencrypted `/tmp`
  profile before constructing a browser.
- The separate gocryptfs test vault passed write/read and normal unmount/removal.
  A synthetic process holding a vault file and ignoring SIGTERM was terminated
  before normal unmount. The existing installed vault was not accessed.
- Session policy unit tests reject locked, inactive, closing, unknown and
  sleep-preparing states, and fail on missing monitor information.

`live_test.py` creates a disposable encrypted vault with a random in-memory key,
uses logind session checks and a sleep delay inhibitor, and supervises the browser
process group. Account data and XDG cache/config/data directories stay inside
that mount. The browser requires a gocryptfs-backed path and the launcher parent;
remote pages receive no controller IPC capabilities. Native Chromium prompts
handle camera/microphone consent; other permissions and new windows are denied.
Closing the window, losing session safety, or the 15-minute limit triggers process
termination followed by normal unmount. A failed unmount preserves the recovery
directory and reports failure; it never uses lazy/forced unmount.

The user authorized a test account and, after the supervised live test, reported
that WhatsApp calls worked. This is user-observed live calling success; the reply
does not separately enumerate voice/video or incoming/outgoing scenarios, so the
full acceptance matrix remains unverified. The launcher subsequently exited with
code 0 and confirmed that test processes stopped and the encrypted test vault
was unmounted and removed. No installed account profile was migrated.
Integrating the production controller, password/PIN,
catalog, multiple accounts, notifications and AppImage updater remains separate
work; their existing implementation has not been replaced by this fixture.

## Calling runtime investigation — 2026-11-08

The installed desktop entry points to `~/.AppImages/elo_talks.appimage`.
Its desktop-entry cache says 0.5.12, but the desktop file inside the actual
AppImage reports **0.5.13**. The inspected image SHA-256 is
`06fbae85019c37c631fecf178137d4fe8844b120675c40cd9401f32885109eae`.
No installed session or vault was opened or modified.

Extracting the package with `--appimage-extract` found its bundled
`usr/lib/libwebkit2gtk-4.1.so.0` and WebKit subprocesses. Calling that library's
`webkit_get_{major,minor,micro}_version` functions returned **2.52.6**.
The host's 2.54.2 library is therefore not evidence of this package's runtime.
No Elo process was running during inspection, so live process mappings were
not available to check for external launch-environment overrides.

A synthetic GTK/WebKit probe loaded the extracted libraries and subprocesses,
using an isolated Broadway display, an ephemeral website data manager, temporary
XDG directories and WebKit sandboxing enabled. Both `enable-webrtc` and
`enable-media-stream` read back as true. A synthetic HTTPS document returned:

```json
{"secure":true,"peer":"undefined","media":"function","error":"ReferenceError: Can't find variable: RTCPeerConnection"}
```

Its default user agent advertises `AppleWebKit/605.1.15` and
`Version/60.5 Safari/605.1.15`. The absence of `RTCPeerConnection` is an actual
engine capability failure, independent of that browser label. Changing the user
agent or granting camera access cannot supply this missing API. The earlier
permission fix does not make this packaged runtime capable of WebRTC calls.

For comparison, installed Chrome 157, headless with its default sandbox and a
fresh temporary profile, exposed `RTCPeerConnection` and successfully created a
data-channel SDP offer. This verifies the basic API only: neither test accessed
WhatsApp, captured real devices, or established a voice/video call.

### Migration assessment

The project locks Tauri **2.12.5** and directly uses GTK/WebKit APIs for account
views, permissions, notifications, clipboard, avatars, readiness and layout.
There is no supported configuration-only Chromium/Firefox switch in this setup.

- **Electron/Chromium: rejected by the maintainer; do not pursue.**
  The initial assessment below is retained only for comparison.
  Retain the Rust controller, vault, lock/session monitoring, server sync and
  signed updater. Port the trusted local shell and account views to
  `WebContentsView`, with a separate `session.fromPath` under the mounted vault
  for each account. Remote views need sandboxing, context isolation, no Node
  integration and no privileged preload bridge. The trusted shell alone may
  forward bounded IPC operations to the controller. Preserve the existing
  peer UID/PID checks by making the supervised Electron main process the IPC
  client; never widen access to arbitrary same-user processes.
- **Tauri 3 + CEF: preferred investigation path while retaining Tauri.** The
  official documentation currently marks it alpha. The native WebKit adapters
  still need replacement, and Linux packaging/sandbox and lifecycle behavior
  need qualification. Do not treat an alpha framework upgrade as a small fix.
  Require `SandboxPolicy::Required`: the documented default `Auto` may disable
  sandboxing for AppImages on Linux systems restricting user namespaces. A
  candidate that cannot start sandboxed is not acceptable for this project.
  Chromium is bundled through CEF; Electron is not involved.
- **Tauri 2 + custom WebKitGTK:** earlier WebKit releases required experimental
  build features for WebRTC. A custom build is a research option, not a verified
  calling solution; it adds responsibility for engine builds and security
  maintenance. Updating to 2.54 does not solve it: that series disables WebRTC
  during the transition to a LibWebRTC backend. Do not downgrade or replace
  the installed runtime on the assumption that a build flag ensures WhatsApp
  compatibility.
- **Qt WebEngine:** Chromium with documented WebRTC support, but introduces Qt
  and another UI/native integration port. It offers no configuration-only
  migration from this Tauri application.
- **Firefox:** Mozilla's maintained GeckoView embedding path targets Android.
  Launching Firefox externally is possible in principle, but does not preserve
  the current embedded account views and controller lifecycle. It is not the
  preferred engine replacement for this Linux app.

Before replacing production: prototype Chromium with synthetic profiles and
verify media permissions/denial, audio/video loopback, account isolation,
background notifications, clipboard, and complete child-process exit before
vault unmount. Keep every persistent Chromium cache/profile within the vault;
closing a view alone is not proof that its storage processes have exited.
Treat Chromium sessions as new profiles and plan to relink WhatsApp; do not
copy WebKit cookies or IndexedDB into Chromium or erase the old profiles.
Then validate incoming/outgoing calls with an authorized test account in the
packaged AppImage. Engine migration removes this API blocker but cannot by
itself guarantee WhatsApp rollout eligibility or end-to-end calling success.

This investigation changes no production runtime or dependencies and publishes
no release. Sources:
[Tauri runtimes and alpha status](https://v3.tauri.app/develop/webview-runtime/),
[Electron views](https://www.electronjs.org/docs/latest/api/web-contents-view),
[Electron sessions](https://www.electronjs.org/docs/latest/api/session),
[Electron security](https://www.electronjs.org/docs/latest/tutorial/security/),
[Qt WebRTC](https://doc.qt.io/qt-6.8/qtwebengine-features.html),
[GeckoView](https://firefox-source-docs.mozilla.org/mobile/android/geckoview/consumer/geckoview-quick-start.html).

## Status — 2026-10-29

**A real-account notification failure was reported in Elo v0.1.17. On 2026-10-30
the user confirmed that a real message notification appeared after the correction,
with generic text.** This is one reported success, not completion of the entire
background test matrix. ZapZap is a source reference only: the user
explicitly declined installing or running it. No runtime A/B comparison or
framework migration has been validated.

The user supplied Safe diagnostics from v0.1.17 after a new message arrived in a
background account without a desktop notification. The vault was unlocked and Do
Not Disturb was off. All three accounts reported `page: ready`,
`nativeNotifications: 0`, `notificationDelivery: none` and
`pendingNotifications: 0`. Thus no native notification was observed by Elo during
that execution. This does not by itself identify the browser API WhatsApp used.

The accompanying GNOME card has playback controls and the `(9) WhatsApp` title.
It is a media-session control, not an Elo message notification. Its existence
does not contradict the zero native-notification counters.

The user confirmed a generic GNOME notification banner. The existing isolated
WebKit test receives synthetic notifications while the account and window are
hidden. Neither observation establishes WhatsApp message delivery.

Native application control is unavailable in this tool session. Account login,
switching, sending test messages and observing banners require the user's desktop
interaction. No conversation contents, cookies or login tokens have been collected.

## Reproducible inputs

| Input | Elo Talks | ZapZap |
|---|---|---|
| Release | [v0.1.17](https://github.com/ItsAnunesS/elo-talks/releases/tag/v0.1.17) | [7.5.1](https://github.com/rafatosta/zapzap/releases/tag/7.5.1) |
| Source revision | `1c06a1c10c2e13a708364ab41c3e38977884d235` | `ec5159f884882d90490513740225bf1f9acb485a` |
| Download size | 95,173,112 bytes | 207,947,072 bytes |
| AppImage SHA-256 | `9161f015ca54b179d0e284f300572325f45ca2a8a316faa6b942618c1a9275e1` | `5b09b075fec5354834865598d6e69f62967f8fda36aae7448154f63bf74256c7` |
| Engine | WebKitGTK; release package inventory reports 2.52.6 | QtWebEngine/Chromium; exact bundled runtime version not yet established |
| Live test | Failure reported on v0.1.17; retest pending | Excluded by user request |

Both official release downloads were checked against GitHub asset digests. The
unused ZapZap download was removed after the user clarified the scope. Elo remains
under the ignored `dist/integration/` directory; its hash also matches the
published checksum manifest. The Sigstore bundle
was downloaded; independent cryptographic verification was not performed during
this evaluation. Package size is not a measurement of RAM, CPU or responsiveness.

The Elo [build and publication run](https://github.com/ItsAnunesS/elo-talks/actions/runs/40966024311)
completed successfully. An earlier inspection found v0.1.15 at the desktop
launcher's target, but the user's subsequent diagnostics confirm the active test
used **v0.1.17**. Exit through the tray before starting a different build; closing
the window can leave the old process running.

## Source comparison

All ZapZap observations refer to the pinned release revision above. Its source
was inspected as a reference, not copied into Elo Talks.

| Integration boundary | Elo Talks v0.1.17 | ZapZap 7.5.1 | Implication |
|---|---|---|---|
| Incoming notification | WebKit `show-notification` event | Qt `setNotificationPresenter` callback | Both depend on WhatsApp Web producing a browser notification. Neither path is an official WhatsApp messaging SDK. |
| Permission | Initializes the WhatsApp notification origin before page load and grants notification requests there | Grants `Feature.Notifications` in the page permission handler | The permission popup closing automatically can be a grant; it is not proof of failure. Actual delivery must be measured. |
| Account switching | Hides existing webviews; no teardown or reload on selection | Switches the current widget and retains enabled account runtimes | Source code alone does not establish long-term background connectivity. |
| Background scheduling | No application-level keepalive or polling workaround | Background throttling defaults to enabled; an option disables Chromium timer throttling and renderer backgrounding | Test default settings first. Record any later tuning as a separate experiment. |
| Badges | Counts native notification events since the account was last viewed | Extracts a numeric count from page title changes | Counts have different meanings. A badge difference is not evidence of a notification transport failure. |
| Notification click | Opens the corresponding account; closes the original WebKit notification immediately | Activates the account and calls the original browser notification's `click()` | Elo lacks click propagation to WhatsApp. Conversation routing needs an implementation change and an actual acceptance test. |
| Linux delivery | Asynchronous `org.freedesktop.Notifications` calls with fixed generic text | Freedesktop backend for AppImage, portal backend for Flatpak | A desktop service accepting a request does not establish that a banner appeared. |
| Notification content | Sender, message and image are not extracted | Optional sender/message and icon integration | Compare delivery independently of these different privacy choices. |

Relevant source locations:

- Elo: `src-tauri/src/account_permissions.rs`, `account_events.rs`,
  `account_runtime.rs`, `desktop_notifications.rs`, `ui.rs` and `ui_layout.rs`.
- ZapZap: [profile setup and title counter](https://github.com/rafatosta/zapzap/blob/ec5159f884882d90490513740225bf1f9acb485a/zapzap/features/browser/web/web_view.py),
  [permission handling](https://github.com/rafatosta/zapzap/blob/ec5159f884882d90490513740225bf1f9acb485a/zapzap/features/browser/web/page_controller.py),
  [account lifecycle](https://github.com/rafatosta/zapzap/blob/ec5159f884882d90490513740225bf1f9acb485a/zapzap/features/browser/shell/browser_controller.py),
  [notification routing](https://github.com/rafatosta/zapzap/blob/ec5159f884882d90490513740225bf1f9acb485a/zapzap/features/notifications/notification_service.py),
  [native click callback](https://github.com/rafatosta/zapzap/blob/ec5159f884882d90490513740225bf1f9acb485a/zapzap/features/notifications/freedesktop_notification_backend.py),
  [background settings](https://github.com/rafatosta/zapzap/blob/ec5159f884882d90490513740225bf1f9acb485a/zapzap/core/environment/setup_manager.py).

## Confirmed permission inconsistency

The isolated native regression on WebKitGTK 2.52.6 reproduced two different
answers for the same WhatsApp origin: `Notification.permission` was `granted`,
but `navigator.permissions.query({name: "notifications"})` returned `prompt`.
Elo handled initial notification permissions and permission requests, but omitted
WebKit's separate `query-permission-state` signal.

The correction grants notification queries for the exact HTTPS WhatsApp origin
only; other queries reaching this handler are denied. WebKit can mask a denied
result as `prompt` before an origin has requested permission. The test therefore
checks that unrelated origins and geolocation are never granted; Elo's existing
request handler continues to deny non-notification access.
A small owned GLib
boxed wrapper uses the existing Rust dependency's FFI because webkit2gtk 2.0.2
omits the high-level binding for this signal. It adds no dependency, page script,
polling, remote command permission or persistent storage.

The regression checks both permission APIs, no grant of geolocation, denial for unrelated
origins, and notification events with an account and window hidden. This is a
confirmed integration defect, not yet proof that it caused the real WhatsApp
failure. A real incoming-message retest remains required.

Local validation after the correction: 28 Rust tests passed (one separate FUSE
test excluded), 12 Python tests passed, application Clippy and formatting passed,
and the native fixture passed at 100% and 200% scales with WebKit sandboxing
enabled. Existing GLib vendor maintenance warnings remain unchanged.

References: [WebKit's permission query contract](https://webkitgtk.org/reference/webkit2gtk/2.42.3/signal.WebView.query-permission-state.html)
and [denied-to-prompt conversion in WebKitGTK 2.52.6](https://github.com/WebKit/WebKit/blob/webkitgtk-2.52.6/Source/WebKit/UIProcess/WebPageProxy.cpp#L13048).

## Elo live test procedure

Use a second account or device to send incoming messages to an unmuted test
conversation; messages sent to oneself are not a substitute. Do not keep the
receiving conversation open in another desktop client during the trial.

Keep notifications enabled in WhatsApp, the client and GNOME, with Do Not Disturb
off. Keep Elo's vault unlocked and record the inactivity timeout. A screen lock,
suspend or vault lock intentionally stops sessions; exclude those intervals from
the background test and test recovery separately. Use only synthetic message text.

For each scenario:

1. Record versions, scenario, local start time and relevant notification settings.
   In Elo, copy Settings → General → Safe diagnostics before starting.
2. Set the required account/window state. For the 30-minute trials, leave the
   receiving account hidden for the entire interval before sending anything.
3. Send three incoming test messages, spaced ten seconds apart. Observe banners
   and badges without opening the receiving account. Record the first banner's
   delay; notification grouping means three messages need not produce three banners.
4. After fifteen seconds, record the badge and banner outcome. In Elo, collect
   Safe diagnostics again without restarting, locking or changing the account set.
   Then open the receiver and record whether the messages are already present or
   only appear after foregrounding. Visible arrival alone cannot prove the exact
   time a hidden page synchronized.
5. In a separate delivery, click the banner and record account/workspace routing
   and whether the expected conversation opens. Do not conflate this with delivery.
6. Repeat each background scenario three times. Keep
   failures as evidence; do not silently retry until a passing result appears.

| Scenario | Elo after correction |
|---|---|
| Receiving account selected, another desktop application focused | Pending |
| Different account selected in the same workspace | Pending |
| Different workspace selected | Pending |
| Main window hidden, vault/session kept active | Pending |
| Receiving account hidden for 30 minutes | Pending |
| Main window hidden for 30 minutes | Pending |
| Reverse sender/receiver accounts | Pending |
| Restart and restore sessions, then repeat a background delivery | Pending |
| Click routes to receiving account and expected conversation | Pending |
| Lock stops sessions; unlock restores delivery | Pending |

Store only scenario, times, versions, aggregate diagnostics and observed outcomes.
Do not record message text, phone numbers, chat names, account identifiers, QR
codes, screenshots of conversations or network payloads.

### Interpret Elo diagnostics

Compare `nativeNotifications` before and after within the same UI process.
Diagnostics omit account identities and use an internal order; array positions
are not sidebar positions. A total delta can confirm an event reached the app,
but unrelated incoming traffic makes attribution to the test inconclusive.

| Observation | What it establishes | Next investigation |
|---|---|---|
| No native counter increase | No browser notification was observed during the interval | Confirm WhatsApp settings, permission consistency and real delivery; investigate the browser boundary. It does not alone prove a disconnected session. |
| Counter increases; delivery `muted` or `do-not-disturb` | The event reached Elo and local preferences suppressed the popup | Correct the test's notification preferences. |
| Counter increases; delivery `failed` | An event reached Elo and desktop delivery failed | Inspect the notification adapter/service boundary. |
| Counter increases; delivery `submitted`; no banner | GNOME accepted a request; display was not observed | Inspect compositor policy, notification grouping and display behavior. |
| Counter increases; banner and badge observed | One real notification path worked in the stated scenario | Repeat after idle, window hiding and session restoration. |

`notificationDelivery` is the latest delivery state, not a per-message history.
`pendingNotifications` may reset when an account is viewed. `page: ready` only
means page loading finished; it does not establish WhatsApp login or connectivity.

## Decision criteria

- If Elo receives native events but its popup, counter or click routing fails,
  fix that integration boundary first. Changing frameworks is not required by
  that evidence.
- If repeated trials continue failing before Elo's native event boundary after
  correcting permission handling, investigate browser compatibility and background
  scheduling. ZapZap's source can inform a QtWebEngine proof of concept, but cannot
  serve as runtime evidence that Chromium fixes this account's behavior.
- A QtWebEngine prototype must retain account isolation, the encrypted vault,
  strict separation of remote content from privileged operations, mandatory
  sandboxing and shutdown before unmount. It must pass clean Fedora/Ubuntu
  AppImage acceptance and measured CPU/RAM/idle tests before replacing WebKit.
- Neither browser engine guarantees every WhatsApp feature or stable private
  page internals. Profile-photo extraction remains removed as requested.

**Current recommendation:** finish the real-account trials before selecting a
replacement engine. The source review identifies concrete Elo integration gaps,
but does not establish a WebKit failure or a successful ZapZap runtime result.
