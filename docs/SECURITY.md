# Security model

Elo Talks is an unofficial client that loads WhatsApp Web. WhatsApp provides end-to-end conversation encryption; Elo Talks adds account isolation and local protection at rest.

## Vault

Each account's complete profile, including plaintext cookies and IndexedDB data produced by Chromium, must remain inside the gocryptfs mount. Files outside it contain format metadata, wrapped keys, and settings unrelated to conversations. Hardware key names are stored in vault metadata; do not include sensitive information in those names.

A random 256-bit key, passed to gocryptfs through stdin, unlocks the filesystem. Each unlock method protects a copy of that key with AES-256-GCM, a random nonce, and authenticated context binding it to the vault and method. Argon2id uses 64 MiB, three iterations, and parallelism one. Master passwords must contain at least 12 characters.

The six-digit PIN is combined with a 256-bit secret in the persistent Linux Secret Service, accessed through a DH session. There is no file-based keyring fallback or fixed secret. A PIN cannot protect against an attacker who controls the unlocked Linux session and obtains this secret: six digits remain a small search space. Interface delays do not defend against offline attacks.

Hardware keys require FIDO2 hmac-secret, user presence, and user verification. Connect only one key during an operation. The authenticator secret unwraps a key envelope; an authentication signature is not used as an encryption key. The hardware helper is canceled when the Linux session becomes unsafe. The master password remains a recovery alternative. There is no recovery server.

## Local encrypted backups

Backup export is available only after confirmed normal vault closure, with no
account processes running. The current master password authorizes export; a
separate password protects a random archive key using the existing Argon2id
parameters. AES-256-GCM authenticates bounded archive records, their order and
header, including an explicit final marker. Profiles remain gocryptfs ciphertext
throughout export, validation and import staging. Backup archives do not contain
the original password envelope, PIN envelope or hardware-key enrollments.

Import is restricted to an installation without a vault. It authenticates the
entire archive, rejects unsafe paths and excessive sizes, and atomically publishes
the prepared directory. The backup password becomes the new master password.
Import does not merge accounts or synchronize devices, and preserving WhatsApp
login after machine migration remains an unverified compatibility requirement.
A backup is an additional recoverable copy of account credentials: protect its
password and revoke linked devices from the phone if that copy is compromised.
Screen/session safety changes cancel background backup work without opening the
vault. Interrupted operations may leave private ciphertext temporary files.

## Limitations

- Decrypted content exists in memory and the mounted filesystem while the vault is open.
- Root access, malware running as the user, unencrypted swap or hibernation, and compromised WhatsApp/Chromium are outside the at-rest protection model.
- Exported files are outside the vault. Users are responsible for these copies.
- Notification details are opt-in. Channel-only mode shares the configured channel name; preview mode additionally shares the native WhatsApp notification title and body with the Linux notification service. These copies may remain in desktop history or appear on the lock screen and are outside vault protection. Changing the setting or locking the vault does not erase notifications already accepted by the OS.
- Sizes, file counts, timestamps, and previous versions can expose metadata. Authenticated encryption does not prevent rollback to a complete older snapshot.
- Logical deletion does not guarantee physical erasure on SSDs, snapshots, or backups. Revoke linked devices on the phone when removing an account.
- Process core dumps are disabled. This does not control privileged system capture tools.

## Process boundaries

The controller never loads web content. It accepts RPC only through the private session socket after checking the current interface process's UID and PID. Each remote webview has a separate context and directory, with no Tauri capabilities. Commands also check the local webview's label and URL. Interface processes stop before unmounting. Unmount failure is reported as an error, never as a successfully locked vault.

Account list writes are atomic and synchronized. Removing an account stops processes before deleting its profile. Failed vault creation preserves the key envelope so a partially initialized filesystem does not lose its key.

Chromium's own desktop notification delivery is blocked. A browser-scoped native
CDP observer adapts the document and service-worker notification APIs into Elo's
existing privacy, mute, DND, coalescing and account-click policy. No remote
DevTools port or pipe is exposed. Native execution-context/session identities and
exact origins gate delivery; bounded queues fail closed on overflow. Sender/body
are transient and reach the desktop only in opt-in preview mode. They never enter
account state, logs, diagnostics, frontend events or notification-history files.
The adapter does not read conversation DOM or request notification images.

## Clipboard images

Clipboard paste is handled by Chromium's native text/file/image pipeline after
user input. Elo no longer injects WebKit clipboard events or grants a persistent
clipboard-read capability. Native regressions verify image decoding, malformed
image rejection by the browser decoder and that hidden accounts do not receive
the active account's paste. Clipboard contents and clipboard managers remain
outside vault protection.

## Distribution

Actions are pinned by SHA; Cargo.lock pins transitive dependencies. Builds use cargo-audit to check known vulnerabilities. This does not prove the absence of defects or automatically cover all native components. Published inventories include Rust and Go dependencies and build-system packages.

The Sigstore signature over SHA256SUMS also covers the provenance and zsync files listed in that manifest. Verify the identity and issuer, not just signature validity:

```sh
cosign verify-blob --bundle SHA256SUMS.sigstore.json \
  --certificate-identity 'https://github.com/ItsAnunesS/elo-talks/.github/workflows/release.yml@refs/heads/release' \
  --certificate-oidc-issuer 'https://token.actions.githubusercontent.com' SHA256SUMS
sha256sum -c SHA256SUMS
```

There is no security certification or promise of vulnerability-free libraries. Read VALIDATION.md before using real data.

## Dependency advisories

The former GTK3/GLib 0.18.5 chain required the reviewed `RUSTSEC-2024-0429`
backport in `vendor/glib`. The CEF/GTK4 graph uses upstream GLib 0.22.10; the
historical patch checksum and the runtime regression remain tested. The legacy
patch is not selected by the new dependency graph. New unsoundness advisories
still block publication.

The audit also reports unmaintained dependencies: `fxhash`, `zbus_names`, `zvariant`, and `zvariant_derive`. These remain visible maintenance debt; they are not described as proven safe libraries.

The helper is built from a pinned gocryptfs 2.6.1 revision with explicit updates to `x/crypto`, `x/sys`, and `x/term`. Its crypto, content, and filename tests run before packaging. `govulncheck` uses the same build tags: an OpenPGP advisory exists in x/crypto, but that package is neither imported nor reachable. No Go audit exception is used. Packaging tool versions and checksums are pinned; a changed mutable download must fail verification until reviewed.

The local `tauri-runtime-cef` 3.0.0-alpha.7 source comes from upstream commit
`81bfb7ca8f1a17be1e48c9b0d167c77b2137f9d3`. Its focused patch rejects remote
page fullscreen in windows containing multiple child views, preserving trusted
account and vault controls. Its CEF bindings are updated to 156.4.0 and the
runtime is pinned to CEF 156.0.26 / Chromium 156.0.8078.88 with an archive SHA-256.
The adapter also rejects invalid request headers without panicking; shared
regressions exercise that boundary. Both upstream runtime licenses are retained
and bundled.

The build-only `phf_generator` 0.8.0 source is retained from upstream commit
`81c7cc5b48649108428671d3b8ad151f6fbdb359`, with only its `rand` requirement
raised to 0.8.6 to remove the `RUSTSEC-2026-0097` dependency. Its deterministic
`SmallRng` usage and hash-generation implementation are unchanged; generated
selector tables are compiled and exercised through the native fixtures. The
upstream MIT license is retained. No new audit exception is introduced.

## AppImage sandbox

CEF uses `SandboxPolicy::Required`, rejects debugging environment overrides and
remote debugging, and cancels certificate errors without a bypass interstitial.
The package ships no sandbox-disabling flags, setuid helper or host-policy hook.
CEF bootstraps its user namespace with a memory-only default request context;
then the UI becomes nondumpable before opening persistent shell/account contexts
or accepting credentials. Core dumps are disabled from process entry. The
controller and authentication helpers become nondumpable immediately.

Accounts use `chromium/accounts/<UUID>` inside the unlocked vault. Browser
temporary files also stay inside that profile at `.cef-tmp` and are removed only
after browser shutdown, before backup or normal unmount. Legacy
`accounts/<UUID>` profiles remain intact for recovery and require fresh QR linking
in Chromium. The controller requests graceful UI shutdown, then kills remaining
members of the browser process group before normal unmount. Only the exact
Chromium singleton rendezvous links are removed after shutdown so encrypted
backups can retain their prohibition on symlinks and special files. Failure to
close the vault leaves an explicit recovery state.

An unexpected interface failure terminates the application and closes the vault, without automatically restarting windows. The vault helper receives a termination signal if the controller dies. Unmount failures remain visible errors.

## Built-in AppImage updates

The controller checks the public GitHub latest release at startup and every five minutes by default while running (including while the window is hidden in the tray). Startup updates install before unlocking; updates found during use require **Restart to update**. Checks have a 15-second deadline; downloads/verification have a five-minute deadline. Startup verification and available updates are mandatory. Offline or failed checks prevent opening the account UI until a successful retry; the launcher offers quit and respects retry cooldowns. Unsupported launches require the installed AppImage. Vault closure recovery stays in the launcher and cannot bypass startup verification.

The bundled static Go helper uses Sigstore's official verification library, authenticated TUF trust metadata, certificate transparency, transparency-log evidence, and the exact release workflow certificate identity and GitHub OIDC issuer above. It authenticates SHA256SUMS and binds the versioned x86_64 AppImage to its signed digest. HTTPS, bounded downloads, strict release asset origins, and no downgrades are enforced. New releases use the standard Sigstore bundle format; publication verifies the bundle with the actual packaged helper before making the release public. No executable verifier is downloaded at runtime.

The controller closes every profile process and confirms unmount before installation. Installation revalidates the staged signature and digest, preserves the previous executable inside a private `.elo-update-*` directory next to the installed AppImage, and atomically replaces the original path. A restart always begins with a locked vault. The AppImage directory must be writable by the current user; no privilege escalation is attempted. Interrupted downloads may leave a private staging directory; old backups can be removed while the app is closed. These files contain only public release artifacts, never account profiles.

The first version with this updater must be installed through the existing download/Gear Lever flow. Existing older versions cannot acquire this capability without that initial upgrade. All account webviews start after unlock. The last selected account ID is stored inside the encrypted catalog. Startup probes inspect only structural element presence and visibility in an isolated JavaScript world every 500 ms for at most 40 seconds per navigation, stopping early when ready. They return only a fixed readiness code and never read text, input values, QR attributes/pixels, cookies, conversation contents, or private WhatsApp APIs. Readiness is a UI hint, never authorization or proof of message synchronization. Slow or changed pages expose Continue anyway; remote Tauri capabilities remain disabled.

The unread adapter observes the native Chromium document-title event only for the exact HTTPS WhatsApp origin. It accepts only the fixed `WhatsApp` or `(digits) WhatsApp` shape with at most six ASCII digits; arbitrary titles are discarded. Only the numeric value enters runtime state and diagnostics. No conversation access, injected JavaScript, message history, or new remote permissions are introduced. During notification bursts, at most one newest pending bounded notification payload per account is retained in process memory while the current D-Bus request completes.


## Encrypted account card photo cache

The avatar adapter selects only an explicitly labeled self-profile control in the
lower-left WhatsApp navigation. It accepts a unique image source, never conversation
images or private WhatsApp APIs. Native extraction is restricted to the already
loaded HTTPS resource whose URL exactly matches the self control, with PNG/JPEG
MIME and size checks; it makes no additional request. WhatsApp-origin blob images
use normal origin-clean canvas access. A denied canvas remains denied.

The normalized 64×64 PNG is cached as bounded base64 in `self-avatar.json` inside
each account's encrypted profile, never in the catalog, logs or diagnostics. Only
the UI process accesses it after unlocking; locking terminates this process before
vault unmount. The cache stores no source URL. Reads validate and normalize the
raster again. Missing or corrupt cache entries fall back to initials. Reloads keep
the last known photo while the background probe refreshes it; a detected QR sign-in
screen clears it, and removing the account deletes its profile and cache. A cached
photo can be stale until a new self photo or QR screen is detected.

Resource hostnames are not treated as proof of identity: selection
requires the unique self-profile control in the exact WhatsApp origin, and only
that already loaded browser resource is read. No URL is fetched by the native
adapter. The bounded probe stops on success or after seven attempts spaced five
seconds apart (with a 40-second cancellation deadline). Retry reloads the page and
resets this budget; selecting a channel does not. This adapter
does not grant remote pages any Tauri permissions or relax TLS, CORS or sandboxing.

Update failures double the configured interval, up to 60 minutes (10, 20, 40, then 60 minutes with the default). A longer
GitHub `Retry-After` (seconds or HTTP date), or an exhausted quota
`X-RateLimit-Reset`, takes precedence. A successful operation restores the normal
configured schedule. The controller enforces this cooldown for retries while it
is running; restarting the app resets the in-memory schedule.

After installation, the controller stops the temporary UI, releases its listener
and instance lock, removes inherited AppImage mount paths from the launch
environment, and uses Unix `exec` to replace itself with the updated AppImage.
A successful restart cannot resume old controller code. An exec failure is reported
as an installed update that requires manually reopening the app.

Settings → General offers background check intervals of 5, 10, 15, 30 or 60
minutes. This non-sensitive device preference is saved privately in `updates.json`
outside the vault so it is available before unlocking. Changing it cannot shorten
an active error/server cooldown. Restart to update downloads and verifies the available release first, then closes
the vault, replaces the AppImage and restarts. It no longer reopens the old executable
and relies on a second startup check. The new process checks its compiled version
against the expected installed version and reports a mismatch instead of silently
claiming success. A download or signature failure leaves the current app usable.

Safe diagnostics includes `avatar`, `avatarAttempts` and `avatarProbeActive` for
each account, without identifying the account or exposing image URLs/pixels.
Retry reloads the WhatsApp page, preserves the cached photo and starts a new budget
of at most seven attempts. Search stops earlier on success.

## Session and local-state hardening (0.4)

The Linux monitor requires logind's active, unlocked session state and requests
vault closure on inactivity, including fast user switching. Authentication is
followed by a second monitor-state check before account catalog initialization.
Polling still has latency and suspend inhibitors have a system-imposed deadline;
real suspend/lock acceptance is tracked separately in VALIDATION.md.

JSON state reads reject final-component symlinks and nonregular files, and use a
nonblocking open to avoid hanging on FIFOs. Atomic writes zeroize their temporary
serialized buffer. These safeguards do not extend the threat model to a malicious
process already running as the same user or erase copies managed by the OS.

## Server metadata connection

The controller performs bounded HTTPS metadata requests on one background worker.
Remote WhatsApp pages receive no new capabilities or network/vault commands.
Tokens and connection settings are stored inside the encrypted catalog; server
passwords are not persisted. TLS is verified, redirects are rejected, and private
CA trust requires explicit PEM configuration. Profile identifiers are bound to
origin, username, organization, workspace and channel, so remote UUID reuse cannot
select another context's local profile.

Managed structure is server-authoritative and desktop mutations are rejected.
Periodic authorization failure blocks managed views; verified removal is applied
after stopping account processes, with atomic catalog persistence. Unchanged
snapshots do not restart accounts or rewrite state. Linux session monitoring
continues during requests, and results are discarded after locking. This is
bounded polling rather than instantaneous revocation. Previously downloaded data
and user-owned backups cannot be remotely erased by an ACL change. See
SERVER-SYNC.md for lifecycle details and explicit manual acceptance limits.

## Camera and microphone

Account views use Chromium WebRTC. Only the WhatsApp top-level origin may reach
Elo's asynchronous camera/microphone dialog; other permission kinds are denied.
The native requesting frame and exact HTTPS origin are checked before prompting
and again before answering. Navigation and revocation invalidate pending approvals.
Decisions are separate per device/account and persisted atomically inside the
encrypted profile. Missing decisions ask; unreadable state fails closed. Requests
time out after 60 seconds and concurrent prompts per account are refused.
Settings → Security can block or reset either permission. Elo updates Chromium's
origin content setting and reloads the account to terminate existing streams.
Remote pages cannot call the local permission settings commands.
Permission approval grants no Tauri capability and does not change TLS, sandbox
or vault boundaries. Real calling and hardware acceptance must be checked in the
packaged application, separately from synthetic peer tests.
