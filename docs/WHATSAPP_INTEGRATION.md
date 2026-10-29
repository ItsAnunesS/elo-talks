# WhatsApp integration evaluation

## Status — 2026-10-29

**A real-account notification failure was reported in Elo v0.1.17. The fix still
requires validation with WhatsApp.** ZapZap is a source reference only: the user
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
