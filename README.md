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
Elo Talks does not inspect the WhatsApp DOM, page title, conversations or QR codes.

Enable notifications in WhatsApp to receive native notification events. Account
badges and the Overview total count those events since the account was last viewed
in a focused window. Counts continue while another workspace is active or the window
is hidden. They are not exact unread-message totals; WhatsApp can group or suppress
notifications. Counters exist only in memory and reset when the vault closes.

Settings offers global **Do not disturb** and per-account muting. These suppress
new desktop popups while retaining counters. Preferences are saved inside the vault.
Desktop notifications contain only a generic message, and clicking opens the account.
At most four notification action handlers wait concurrently; extra events still
update the badges. Already delivered desktop notifications are controlled by the OS.

The window title includes the pending total. Compatible docks can also receive a
Unity LauncherEntry numeric badge for the desktop entry. GNOME Shell does not
universally support this protocol; dock extensions and AppImage manager naming can
affect display. No extension is installed or security setting changed by the app.

**Safe diagnostics** in Settings shows a local, selectable report with app version,
platform, page states, counters and vault state. It excludes message content, account
names/identifiers, credentials, cookies, filesystem paths and remote URLs. Nothing
is uploaded automatically. TLS failures never create certificate exceptions.
