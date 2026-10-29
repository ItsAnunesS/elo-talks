# Elo Talks

A Linux desktop application built with Tauri 2 for multiple WhatsApp Web accounts, with persistent profiles stored in an encrypted local vault.

**Status: initial implementation under validation.** An AppImage build does not certify security or platform compatibility. See [validation requirements](docs/VALIDATION.md) for outstanding acceptance criteria.

## Usage

Download the AppImage from Releases, verify its signature and checksums using [SECURITY.md](docs/SECURITY.md), make it executable, and open it. The target platforms are Fedora 44 and Ubuntu 24.04 x86_64 without additional package installation. Clean-system certification remains pending.

On first launch, create a master password with at least 12 characters. Add accounts and scan each QR code with your phone. Accounts are isolated, with optional device PIN, compatible hardware key, and idle timeout. The master password remains a recovery alternative. Without it or another enrolled method, the vault cannot be recovered.

Closing the window keeps the application in the tray when a supported tray is available. Otherwise, the window stays accessible. Screen lock and suspend close account sessions and the vault. Export attachments only to locations where you intend to keep a copy outside the vault.

Autostart records the original AppImage path. Keep the file at that location; after replacing or moving it, launch the new AppImage once to refresh the entry. The autostart file is `~/.config/autostart/dev.anuness.elotalks.desktop`; desktop settings can disable it. Launching the new version removes the previous application ID's entry.

## Updates through AppImage managers

New AppImages include standard `gh-releases-zsync` metadata pointing to the latest stable release of `ItsAnunesS/elo-talks`. Gear Lever and compatible managers can discover updates without manually configured URLs. Every release publishes an AppImage and its matching `.AppImage.zsync`, both covered by `SHA256SUMS` and the Sigstore signature of that manifest.

If the installed version lacks this metadata, replace it once with a new AppImage and reload its metadata in Gear Lever. Use the default source without a manual override. Choose **Quit** before replacing the executable; closing the window may leave the application in the tray. Account data stays in the vault, separate from the executable.

Updates are managed externally; Elo Talks has no built-in updater. Recognition of `.zsync` does not imply that a manager verifies Sigstore automatically. Signature verification is documented in [SECURITY.md](docs/SECURITY.md).

## Development

Build-only dependencies: Rust from `rust-toolchain.toml`, Go 1.26.10, a C compiler, pkg-config, WebKitGTK 4.1, GTK3, libfido2, libdbus, AppIndicator, binutils, and zsync. The frontend uses local HTML, CSS, and JavaScript without runtime npm dependencies or remote scripts.

```sh
bash scripts/prepare-helper.sh
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
python3 -m unittest discover -s tests -v
cargo install tauri-cli --version 2.12.4 --locked
bash scripts/prepare-bundler.sh
bash scripts/build-appimage.sh
```

For development, build with `cargo build`, place the helper in the configured location, and run `target/debug/elo-talks` in a graphical Linux session. The controller requires logind and FUSE. Never disable the sandbox to bypass platform incompatibilities.

See [CONTRIBUTING.md](CONTRIBUTING.md) for language, formatting, and commit conventions.

## Structure

- `ui/`: local interface, account list, authentication, and settings.
- `src-tauri/src/`: controller, vault, key envelopes, authentication, session monitor, and WebKit integration.
- `vendor/glib/`: documented upstream security backport.
- `.github/workflows/release.yml`: tests, audit, AppImage build, and version `0.1.<run_number>` on each push to `release`.

Each account has its own WebKit context and directory. Remote content receives no Tauri capabilities. Local requests are checked by origin and by a private socket validating the caller's UID and PID. There is no backend, telemetry, voice/video calling, or built-in updater.

WhatsApp is a trademark of its owner. This project is not affiliated with WhatsApp or Meta.

Release credentials and recovery are documented in [RELEASING.md](docs/RELEASING.md).

## Build and runtime performance

CI caches the pinned Rust CLI binaries separately from compiled application dependencies. Cache keys include the toolchain, dependency manifests, and vendored source. Go build/module caches and npm downloads are also reused. AppImage outputs, signatures, vault data, and vulnerability results are never cached. Both Rust and Go vulnerability checks still run on every build.

Packaging downloads have their own pristine cache: checksums are verified on every use, then fresh copies are staged for Tauri, which modifies linuxdeploy in place. Compressed AppImages are uploaded without redundant ZIP compression. The first run after a cache-key change remains a cold build; later runs can reuse that work.

The measured baseline before these changes was approximately 14 minutes for the full GitHub workflow: 8m06s for tool installation and AppImage building, 1m54s for audit installation/execution, and 2m16s for checks. Local warm-cache checks took 0.20s to validate installed Rust tools and 0.13s to stage verified packaging tools. These are individual local measurements, not a promised CI duration.

Account switching updates selection classes without rebuilding account buttons or cards. Native input activity uses one bounded background queue instead of synchronous vault RPC on the GTK thread. Inactive accounts stay connected for notifications. Cryptographic parameters, screen-lock monitoring, and sandbox enforcement are unchanged.

## Account health, notifications and diagnostics

Each account displays native WebKit page state and offers **Retry** without deleting
its profile. Load failures, TLS failures, stopped web processes and an unavailable
sandbox are separate states. A successful page load means the WhatsApp page is
available, not that a QR code has been scanned or its messaging connection is healthy.
Health detection does not inspect the WhatsApp DOM, page title, conversations or QR codes.

Enable notifications in WhatsApp to receive native notification events. Account
badges and the Overview total count those events since the account was last viewed
in a focused window. Counts continue while another workspace is active or the window
is hidden. They are not exact unread-message totals; WhatsApp can group or suppress
notifications. Counters exist only in memory and reset when the vault closes.

Settings offers global **Do not disturb** and per-account muting. These suppress
new desktop popups while retaining counters. Preferences are saved inside the vault.
Desktop notifications contain only a generic message, and clicking opens the account.
A selected account can contain other chats, so window focus does not discard native
notification events. WhatsApp decides when to emit them; app muting still applies.
Delivery uses asynchronous GIO/D-Bus calls with a three-second timeout and one
retained notification per account. Clicks and dismissal arrive as native signals;
there are no threads waiting for notification clicks. Bursts share an in-flight
request, while all native events still update counters. Already delivered desktop
notifications are controlled by the OS.

The account context initializes notification permission only for
`https://web.whatsapp.com`; other feature requests remain denied. WhatsApp's own
notification settings still apply. **Settings → Notifications → Test Linux
notification** tests desktop delivery independently of WhatsApp and app muting.
A successful test means the desktop service accepted it; GNOME may still hide banners.
Safe diagnostics include `nativeNotifications` and `notificationDelivery` per account:
`none`, `muted`, `do-not-disturb`, `pending`, `submitted` or `failed`.
These counters and fixed status codes contain no sender, message, URL or timestamp
and reset with the UI process. `submitted` confirms desktop acceptance, not visibility.

The window title includes the pending total. Compatible docks can also receive a
Unity LauncherEntry numeric badge for the desktop entry. GNOME Shell does not
universally support this protocol; dock extensions and AppImage manager naming can
affect display. No extension is installed or security setting changed by the app.

**Safe diagnostics** in Settings shows a local, selectable report with app version,
platform, page states, counters and vault state. It excludes message content, account
names/identifiers, credentials, cookies, filesystem paths and remote URLs. Nothing
is uploaded automatically. TLS failures never create certificate exceptions.

## Account initials

Account cards display initials derived from the channel name. Elo Talks does not
inspect WhatsApp profile controls, poll for profile photos or make avatar requests.

## Desktop icon refresh

On each AppImage launch, Elo Talks refreshes matching launchers in the user's
`$XDG_DATA_HOME/applications` directory (normally `~/.local/share/applications`).
The bundled application logo is copied to `elo-talks/desktop-icons` under the same
data directory with its SHA-256 in the filename. A logo change therefore gives
GNOME a new icon path, instead of reusing a cached image under the old name.

Only existing, user-owned Elo Talks launchers targeting the current AppImage are
updated. Their filename, Exec command, manager metadata, translations and actions
are preserved. No duplicate launcher is created, no manager-owned icon is replaced,
and no desktop restart or cache-deletion command is needed. Unchanged launchers
are not rewritten. An update manager can regenerate its launcher; opening the
updated AppImage reapplies the icon automatically. Integration errors are nonfatal
and retried on the next launch. This handles the application logo, not account photos.

## Workspaces and channels

The left rail switches between workspaces. Each workspace has channels at its root
and optional collapsible categories. WhatsApp is the only supported channel service;
its icon precedes channel names. Existing accounts migrate into **My workspace** on
first unlock, keeping account IDs and profile directories unchanged. Moving a channel
changes its organization only and does not require scanning the QR code again.

Use **Add workspace** on the rail, the workspace name to manage it, **+** beside
Channels to connect WhatsApp, and **Add category** to create a group. The channel menu
supports renaming, changing workspace/category, ordering within its group, and removal.
Deleting a category moves its channels to the workspace root. A workspace must be
empty before deletion, and at least one workspace remains. Background accounts stay
running; workspace badges sum native notification counters across their channels.
Clicking a notification switches to the correct workspace and expands its category.

Workspace images are chosen by the user. PNG/JPEG/WebP files (up to 5 MB and
4096×4096) are cropped to 64×64 RGBA in the local interface. The controller accepts
only exactly sized pixels and encodes a fresh PNG
as base64 inside the encrypted vault. Original image metadata, animation and file paths
are not stored. SVG and remote image URLs are not accepted. Image payloads are loaded
separately from status metadata to retain the existing 64 KiB IPC limit. This adds no
remote-webview permissions or dependencies.

Organization is atomically saved in `catalog.json` inside the vault (up to 20 workspaces,
40 categories and 20 accounts). The legacy `accounts.json` is retained for recovery but
is no longer used once the catalog exists; older app versions do not understand this
catalog, so downgrading is not supported. Categories and channel assignments persist;
category collapse state is kept in memory. Synthetic migration, persistence and browser
flows are tested; real-account and clean-system certification remains separate.


## Desktop navigation and dialogs

**Find a channel** searches channel, workspace and category names locally, without
network requests. Use the arrow keys and Enter in the search dialog. Ctrl/Cmd+K
opens it while focus is in the Elo Talks interface; WhatsApp's own webview keeps
its keyboard behavior. Newly created channels open automatically for sign-in.
Categories also offer an Add channel action with their location already selected.

Settings are grouped into General, Notifications and Security. Dialogs keep actions
visible while content scrolls, retain input after save errors, and prevent duplicate
submissions. Destructive account/workspace actions require an inline confirmation.
Password fields clear when leaving the Security tab or closing the dialog. Success
feedback stays inside the active dialog; other notices use the sidebar so a native
WhatsApp webview cannot cover them. A loaded page is not presented as proof of login.

The UI uses local vector icons, system fonts and reduced-motion support; no new
runtime dependencies, remote assets, content permissions or vault changes are required.
Browser regressions cover 900/1200/1600px windows, keyboard search, form errors,
confirmations, pending saves and the fixed 304×72 native account viewport offsets.
