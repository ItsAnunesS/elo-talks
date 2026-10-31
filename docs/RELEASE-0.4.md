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
