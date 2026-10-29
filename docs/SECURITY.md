# Security model

Elo Talks is an unofficial client that loads WhatsApp Web. WhatsApp provides end-to-end conversation encryption; Elo Talks adds account isolation and local protection at rest.

## Vault

Each account's complete profile, including plaintext cookies and IndexedDB data produced by WebKit, must remain inside the gocryptfs mount. Files outside it contain format metadata, wrapped keys, and settings unrelated to conversations. Hardware key names are stored in vault metadata; do not include sensitive information in those names.

A random 256-bit key, passed to gocryptfs through stdin, unlocks the filesystem. Each unlock method protects a copy of that key with AES-256-GCM, a random nonce, and authenticated context binding it to the vault and method. Argon2id uses 64 MiB, three iterations, and parallelism one. Master passwords must contain at least 12 characters.

The six-digit PIN is combined with a 256-bit secret in the persistent Linux Secret Service, accessed through a DH session. There is no file-based keyring fallback or fixed secret. A PIN cannot protect against an attacker who controls the unlocked Linux session and obtains this secret: six digits remain a small search space. Interface delays do not defend against offline attacks.

Hardware keys require FIDO2 hmac-secret, user presence, and user verification. Connect only one key during an operation. The authenticator secret unwraps a key envelope; an authentication signature is not used as an encryption key. The hardware helper is canceled when the Linux session becomes unsafe. The master password remains a recovery alternative. There is no recovery server.

## Limitations

- Decrypted content exists in memory and the mounted filesystem while the vault is open.
- Root access, malware running as the user, unencrypted swap or hibernation, and compromised WhatsApp/WebKit are outside the at-rest protection model.
- Exported files are outside the vault. Users are responsible for these copies.
- Sizes, file counts, timestamps, and previous versions can expose metadata. Authenticated encryption does not prevent rollback to a complete older snapshot.
- Logical deletion does not guarantee physical erasure on SSDs, snapshots, or backups. Revoke linked devices on the phone when removing an account.
- Process core dumps are disabled. This does not control privileged system capture tools.

## Process boundaries

The controller never loads web content. It accepts RPC only through the private session socket after checking the current interface process's UID and PID. Each remote webview has a separate context and directory, with no Tauri capabilities. Commands also check the local webview's label and URL. Interface processes stop before unmounting. Unmount failure is reported as an error, never as a successfully locked vault.

Account list writes are atomic and synchronized. Removing an account stops processes before deleting its profile. Failed vault creation preserves the key envelope so a partially initialized filesystem does not lose its key.

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

`RUSTSEC-2024-0429` affects GLib 0.18.5, required by Tauri's GTK3 dependency chain. The project vendors the exact upstream fix under `vendor/glib`, with a regression test and checksum of the patched file. The audit exception applies only to this locally corrected advisory. New unsoundness advisories block publication.

The audit also reports unmaintained dependencies: `proc-macro-error`, `zbus_names`, `zvariant`, and `zvariant_derive`. These remain visible maintenance debt; they are not described as proven safe libraries.

The helper is built from a pinned gocryptfs 2.6.1 revision with explicit updates to `x/crypto`, `x/sys`, and `x/term`. Its crypto, content, and filename tests run before packaging. `govulncheck` uses the same build tags: an OpenPGP advisory exists in x/crypto, but that package is neither imported nor reachable. No Go audit exception is used. Packaging tool versions and checksums are pinned; a changed mutable download must fail verification until reviewed.

## AppImage sandbox

WebKitGTK 4.1 requires explicit sandbox activation. The controller sets `WEBKIT_FORCE_SANDBOX=1`, removes sandbox bypass variables, and only loads remote content after confirming the WebKit context is sandboxed. The bundle includes bubblewrap and xdg-dbus-proxy. `scripts/bwrap.sh` adapts linuxdeploy's relative paths, preserves namespaces and seccomp, and mounts the bundle and `/usr` read-only. These system libraries are normally exposed by WebKit's sandbox. The adapter does not mount the user's home directory or the vault wholesale.

An unexpected interface failure terminates the application and closes the vault, without automatically restarting windows. The vault helper receives a termination signal if the controller dies. Unmount failures remain visible errors.
