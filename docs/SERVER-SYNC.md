# Server metadata synchronization — 0.5.x

## Connect

1. Create or unlock the local encrypted vault. It still protects this computer's
   WhatsApp profiles independently of server login.
2. Open Settings > General > Your server.
3. Enter the HTTPS domain, server username and password, then choose
   **Connect and sign in**. The API must be hosted at that domain's root.
4. Choose an organization and **Use organization**. A single assigned organization
   is selected automatically. Ask your administrator if none is assigned.
5. Authorized workspaces/categories/channel slots appear alongside local
   workspaces. Manage their structure and access in the server panel. Local
   workspace editing continues normally. Keep at least one local workspace for
   offline use.

Public certificates use normal trust validation. For a private/localhost CA,
expand **Private certificate** and supply the public PEM obtained through a
trusted administrator channel. Never paste a private key or disable certificate
validation. Previously configured trust can be reused for sign-in to the same
origin. HTTP, embedded credentials, paths, query parameters and redirects are
rejected.

## What is synchronized

Names, categories, channel slots and workspace visibility. The server is the
source of those managed workspaces; this first integration is not bidirectional
editing of server structure from the desktop. Managers can edit a granted
workspace through the panel if they also have a manage grant.

WhatsApp cookies, IndexedDB, QR codes, messages and complete profiles stay local.
Each computer links its own WhatsApp session. Server login is not a replacement
for linking WhatsApp on a new PC. Local-only workspaces are never uploaded.
The encrypted backup feature remains available independently.

Tokens are saved only in the encrypted catalog, not shell storage outside the
vault. Passwords are cleared after sign-in and are not persisted. Catalog schema
2 records a server connection; older clients reject it rather than silently
turning managed workspaces into local ones. Local-only catalogs remain schema 1.
Back up before downgrading; use a current 0.5.x build for server catalogs.

## Lifecycle and permissions

- One worker performs HTTPS requests; the controller continues monitoring Linux
  lock/suspend/inactivity while network requests run. In-flight results are
  discarded after the vault is locked or the session becomes unsafe.
- Access is checked at startup/unlock and every 30 seconds, with per-request
  timeouts. Revocation is not instantaneous: allow the polling interval plus
  in-flight requests (up to roughly one minute under timeouts).
- Snapshots support ETags. An unchanged snapshot does not restart accounts or
  rewrite the catalog. Actual structure/access changes stop the UI/account
  processes before atomically saving the catalog and reopening authorized views.
- Connection failure or inability to verify access blocks/hides managed views
  and explains recovery in the sidebar/Settings. It does not delete their profiles
  merely because the server is offline. Local workspaces stay available.
- A successfully verified removal/revocation removes managed entries and prunes
  their local profiles after worker shutdown. Switching organization/login or
  disconnecting can therefore remove those managed profiles from this device.
  Disconnect asks for confirmation and preserves local workspaces.
- Channel/profile IDs include the normalized origin, username, organization,
  workspace and remote channel identity. Reusing a remote UUID elsewhere cannot
  select another login/server's profile. Changing origins is a new profile scope.
- Native server structure edits are rejected even if a caller bypasses the UI.
  Notification preferences, reload and local navigation remain device-local.
- Existing desktop bounds apply to local plus server content: 20 workspaces,
  20 channels and 40 categories. Oversized imports leave the existing catalog
  intact and explain that local content must be reduced before syncing.

Changing a server grant is not cryptographic erasure of data the user already
received or backed up. The vault owner and host remain trusted; no remote-wipe
claim is made. Tokens expire after eight hours maximum and require server sign-in
again, without intentionally discarding valid retained WhatsApp profiles.
Disconnect removes the local token and attempts server-side revocation without
blocking vault monitoring. If the server is unavailable, revoke the session in
the panel later; it otherwise expires according to server policy. Signing in
again to the same origin/username retains the selected organization when still
authorized, preserving stable profile IDs even with multiple memberships.

## Validation

Rust tests cover stable namespaced IDs, domain/user/org/workspace separation,
invalid snapshots, local preservation, idempotent imports, token exclusion from
status metadata and rejection of managed edits. Controller tests also reject
server commands while locked. Playwright covers domain/login/organization controls
and secret clearing, with normal/minimum-size captures.

The explicit synthetic integration fixture
`server_sync::tests::syncs_with_real_https_postgres_service` was run against a
real HTTPS server with PostgreSQL, a private trusted certificate and a member
with an explicit workspace grant. It verified the wire contract, catalog import,
an unchanged ETag response, and re-login with multiple memberships without
changing the selected organization/profile IDs or opening WhatsApp. To rerun it, provision a
disposable server with one granted workspace and set `ELO_SYNC_TEST_ORIGIN`,
`ELO_SYNC_TEST_USERNAME`, `ELO_SYNC_TEST_PASSWORD`, `ELO_SYNC_TEST_CA` and
`ELO_SYNC_TEST_ORGANIZATION` (for multiple memberships), then run:

```sh
cargo test --locked server_sync::tests::syncs_with_real_https_postgres_service -- --ignored --exact
```

The normal suite leaves this fixture and the host-FUSE fixtures ignored. Native
WebKit shutdown/revocation on GNOME, real account linking/notifications, offline
network transitions, real public DNS/ACME deployment and an installed AppImage
upgrade remain manual acceptance. No live account was accessed.
