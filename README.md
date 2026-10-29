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
