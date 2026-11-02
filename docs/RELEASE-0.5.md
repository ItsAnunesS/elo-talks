# Elo Talks 0.5 — Local backups and server metadata

This series adds portable local backups on Linux. It preserves the clipboard
image-paste correction from commit `877741b` and the existing vault protections.
The desktop can also connect to a separately deployed server for authorized
metadata. It does not host WhatsApp sessions remotely or claim Windows/macOS support.

## Backup workflow

- Lock the vault normally before exporting. From Settings > General, choose
  **Lock to export backup**, then **Encrypted backup** on the lock screen.
- Enter the current master password and a separate backup password of at least
  12 characters. Choose a new `.elobackup` filename outside the app data directory.
- Keep Elo Talks open until it confirms completion. Backups include the complete
  closed encrypted filesystem: workspaces, categories, channels, images,
  preferences and WebKit account profiles. Application binaries and updater
  staging files are excluded.
- On another Linux installation, choose **Encrypted backup** before creating a
  vault. Select the file and supply its backup password. That password becomes
  the restored vault's master password; it can be changed after unlocking.
- Import never merges into or replaces an existing vault. PIN and hardware-key
  enrollment are excluded and must be configured again on the destination.

A backup is a snapshot, not synchronization. WhatsApp may require relinking;
profile migration between two physical Linux PCs and concurrent use of cloned
sessions have not been accepted. Avoid running original and restored copies of
the same WhatsApp session together.

## Format and failure behavior

Version 1 uses a random AES-256-GCM data key wrapped by the existing Argon2id
credential envelope. Every bounded record authenticates the envelope header and
its sequence position. The authenticated final record and exact EOF check reject
truncated or appended archives. No profile is decrypted into a temporary folder.
Import staging holds only the original gocryptfs ciphertext and a manifest whose
master-key envelope uses the backup password. Restored vaults receive a fresh
internal identity so later PIN enrollment cannot overwrite the source vault's
keyring secret. Account and workspace identifiers are preserved.

Limits: 64 GiB of ciphertext, 200,000 entries, 32 MiB of path metadata, 64 path components, and 1 MiB per
encrypted record. Links, special files, traversal, duplicates and unsupported
format versions are rejected. Export publishes without overwriting an existing
file, using a temporary file and Linux no-replace rename (with a hard-link fallback).
Filesystems supporting neither operation report an error; publication failure
leaves the previous destination untouched.

Import validates the complete archive before atomically publishing its prepared
vault directory. Normal errors clean up staging. A crash may leave private
ciphertext staging files; these contain no plaintext profiles. A crash after
atomic publication leaves a complete vault discoverable at the next startup.
Like the existing vault, the backup cannot defend against a compromised user or
root process, and password loss has no recovery service.

## Version policy

GitHub workflow run 51 was the latest observed run on 2026-11-02. The series is
configured as `0.5.0` starting at run 52; later runs increment its patch version.
Recheck the latest workflow number before publishing if other pushes intervene.
No automated version-bump commits are added.

## Server metadata integration

The next 0.5.x push build adds HTTPS server sign-in and authorized organization,
workspace, category and channel metadata. The local vault and backup remain
independent; WhatsApp profiles are not uploaded or shared between PCs. Server
structure is managed through the server's PostgreSQL-backed administration panel.
See SERVER-SYNC.md for setup, role/access behavior and remaining native acceptance.
The existing run-offset policy continues; no release-series reset is required.
