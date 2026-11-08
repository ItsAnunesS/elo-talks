![Elo Talks — Separate accounts. One private space.](docs/assets/banner.svg)

<p align="center">
  <a href="https://github.com/ItsAnunesS/elo-talks/releases/latest">Download</a> ·
  <a href="docs/SECURITY.md">Security</a> ·
  <a href="CONTRIBUTING.md">Contributing</a> ·
  <a href="https://github.com/ItsAnunesS/elo-talks/issues">Report an issue</a>
</p>

# Elo Talks

A Linux desktop app for managing multiple WhatsApp accounts in one place. Organize
personal and work accounts into workspaces, keep them connected in the background,
and protect their local profiles with an encrypted vault.

Built with **Tauri 3, Rust and Chromium Embedded Framework (CEF)**, without Electron.
The Tauri/CEF prereleases and Chromium distribution are pinned. Server controls are
hidden; local accounts and encrypted backups remain available.

## What it does

- **Workspaces and channels.** Group accounts into categories, pin favorites and
  filter channels with unread activity.
- **Separate sessions.** Each account has its own browser context and persistent
  profile. Switching channels keeps other accounts running.
- **Local protection.** Unlock with a master password, an optional device PIN or a
  compatible FIDO2 key. Account processes stop before the vault is unmounted.
- **Encrypted backups.** Export a closed vault with a separate password and
  restore workspaces and account profiles into a new Linux installation. See
  [backup instructions and limitations](docs/RELEASE-0.5.md).
- **Desktop notifications.** Choose private alerts, channel names or message
  previews. Pause notifications globally or mute individual accounts.
- **Verified updates.** A bundled launcher checks for updates before opening the
  app. During use, updates prepare in the background and wait for you to restart.
- **Keyboard-friendly navigation.** Search channels with `Ctrl+K`, adjust text size
  and density, and respect the system's reduced-motion setting.

## Get started

1. Download the **Linux x86_64 AppImage** from [Releases](https://github.com/ItsAnunesS/elo-talks/releases/latest).
2. [Verify its signature and checksum](docs/SECURITY.md#distribution), make the file
   executable, and open it.
3. Create a master password, add a WhatsApp channel, and link it by scanning the QR
   code from WhatsApp on your phone.

Keep your master password safe. There is no server-side recovery if you lose all
enrolled unlock methods.

**Platform status:** Fedora 44 and Ubuntu 24.04 are the target systems. Clean-system
validation is still pending; a published AppImage is not a compatibility or
security certification. See [validation status](docs/VALIDATION.md).

## Everyday use

Closing the window keeps accounts running when a supported tray is available.
Use **Quit** to exit. Screen lock, session inactivity and suspension request vault
closure; if closure fails, the app shows recovery instead of claiming it is locked.

Enable notifications in WhatsApp as well as Elo Talks. Unread badges use WhatsApp's
counter when available, with a notification-based fallback. They are not guaranteed
message totals. Notification previews may remain in your desktop's history.

The startup launcher requires successful verification and offers retry or quit if it fails. Background
check intervals are configurable in Settings. Gear Lever and compatible AppImage
managers can also discover updates through embedded zsync metadata; that does not
mean those managers verify Sigstore signatures.

## Security boundaries

Profiles are stored in a gocryptfs vault. Remote WhatsApp views receive no Tauri
capabilities, and each account uses an isolated Chromium context.

An unlocked vault cannot protect against a compromised operating system. Exported
attachments, desktop notification history and unprotected swap are outside its
protection.

Account views use Chromium WebRTC with native camera/microphone prompts. Calls
worked in the separate CEF prototype; acceptance in the complete packaged app is
tracked in [validation status](docs/VALIDATION.md).

When upgrading from WebKit, the vault, accounts and old profiles are preserved.
Link each account again by QR: Chromium uses new profiles inside the same vault.
WebKit session databases are not copied into Chromium.

Read the [security model](docs/SECURITY.md) for details and limitations. The
[0.4 release notes](docs/RELEASE-0.4.md) describe the current hardening work.

## Development

Use the pinned Rust toolchain and lockfiles. Native builds require GTK4 >= 4.14,
CEF, libfido2, D-Bus and the packaging tools listed in the
[release workflow](.github/workflows/release.yml). The local interface uses HTML,
CSS and JavaScript with no runtime npm dependencies.

```sh
npm ci --ignore-scripts
export CEF_PATH="$(python3 scripts/prepare-cef.py)"
bash scripts/prepare-helper.sh
bash scripts/prepare-updater.sh
cargo test --locked
npm run format:check
npm run test:ui
python3 -m unittest discover -s tests -v
```

Browser tests require Playwright Chromium. Run `go test ./...` from `updater/`
for the updater suite. Native tests use isolated graphical sessions and synthetic
data; they do not replace testing on supported systems.

| Directory | Purpose |
| --- | --- |
| `src-tauri/src/` | Controller, launcher, vault and native integrations |
| `ui/` | Local interface and account navigation |
| `updater/` | Download, verification and AppImage replacement |
| `tests/` | Browser regressions and repository checks |
| `scripts/` | Build, packaging and release tooling |

See [Contributing](CONTRIBUTING.md), [agent guidelines](AGENTS.md) and
[release instructions](docs/RELEASING.md) before submitting changes.

### Docker and CEF

`Dockerfile.cef` builds the complete app against Ubuntu 24.04, matching CI.
`scripts/prepare-cef.py` verifies the pinned archive's size and SHA-256 before
extracting it. `scripts/build-appimage.sh` uses the existing checksum-pinned
packager, preserves zsync metadata and bundles CEF with the updater and gocryptfs.
The app never disables Chromium sandboxing or installs a host AppArmor exception.
Hosts must permit Chromium's unprivileged user namespaces.

The original `prototypes/tauri-cef` workspace remains a historical calling probe;
it is not the application or the release artifact. See
[the integration record](docs/WHATSAPP_INTEGRATION.md).
