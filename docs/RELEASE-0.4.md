# Elo Talks 0.4 — Security hardening

This series prioritizes verified security fixes. It does not certify the application
or replace the outstanding real-device acceptance checks in VALIDATION.md.

## Initial changes

- Fail closed when logind reports an inactive session, an unknown session state,
  screen lock or suspension. Fast user switching now requests vault closure.
- Recheck session safety after authentication and mounting, before initializing
  account profiles. If it changed, unmount the vault; preserve a visible closure
  error if unmounting fails.
- Reject final-component symbolic links, directories and special files when
  loading JSON state. Open nonblocking before checking file type so a substituted
  FIFO cannot hang the controller. This does not defend against a compromised
  same-user process or all parent-directory replacement races.
- Zeroize the temporary serialized buffer used by atomic state writes.

## Validation gates still open

- Real screen lock, fast user switch, suspend/resume and shutdown with active
  accounts, including delayed authentication and a busy mount.
- PIN/keyring and physical FIDO2 recovery scenarios, abrupt termination and
  disk-full behavior.
- Updated AppImage installation and encrypted-profile restoration on clean
  supported distributions, with the existing sandbox requirements intact.
- Broader isolation, dependency and updater review. Existing unit tests do not
  constitute a completed independent security audit.

Incoming-call experiments are outside this release. Windows/macOS feasibility
remains planned for the 0.5 series.

## Vault closure recovery

Normal unmount now retries up to four times after UI termination to tolerate
brief descriptor-release delays. Each fusermount subprocess has a 400 ms polling
deadline, with 100 ms between attempts. No lazy/forced unmount is used, and master
key state is retained until mount closure is confirmed.

A closure failure opens a dedicated recovery screen instead of a password form.
The action reports pending closure, prevents duplicate submissions, and does not
interpret the RPC acknowledgment as proof of locking. Repeated session-lock events
no longer tear down and restart this recovery interface. Persistent busy mounts
still require recovery; the UI never claims protection that was not confirmed.

## In-AppImage launcher

The controller now starts a separate local GTK launcher process before creating
the Tauri/WebKit shell. It has no account profiles, web content or credential
fields. It uses the existing private PID/UID-checked socket and the same bundled
signature-verifying updater; no external launcher is installed.

Startup checks and preparation display progress. Startup verification and available
updates are mandatory: only a confirmed current version opens the account UI.
Failed or offline checks offer retry (after the controller cooldown) or quit,
never a bypass. Unsupported launches explain that the installed AppImage is
required. Closing the launcher requests normal controller shutdown. Background
updates during an existing session still require an explicit restart request.

Installation disables voluntary shutdown. The controller stops the current interface,
confirms normal vault unmount, and only then starts the installation helper and
launcher progress window. Closure failure offers a dedicated retry-closure action in the launcher. It
stops the launcher, retries normal unmount, retains any closure error and restarts
the launcher; it never opens the account UI or bypasses the required update. The existing expected-version restart check still detects a stale launch.

CI exercises the native launcher under Xvfb with synthetic IPC for current,
verified-ready, failed-update, installation-in-progress and vault-closure failure
states. Actual AppImage replacement and GNOME
window behavior remain release acceptance checks.
