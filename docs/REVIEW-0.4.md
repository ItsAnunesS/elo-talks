# Refinement review — 0.4

## Scope

Reviewed startup orchestration, local navigation/search, update status polling,
notification preferences, JSON persistence, controller recovery and bounded IPC.
This is a focused source review and regression pass, not an exhaustive security
audit or a performance certification of the remote WhatsApp application.

## Corrected findings

| Area | Failure or unnecessary work | Change |
| --- | --- | --- |
| Startup | Failure creating the preferred account prevented remaining accounts from starting | Continue background startup and show Overview with a retry explanation |
| Notifications | A deleted muted account remained in preferences and caused subsequent saves to fail | Drop stale account references on read; retain privacy level and Do Not Disturb |
| Persistence | Writes could exceed the read limit, replacing valid state with unreadable JSON | Validate serialized size before creating a temporary file or replacing state |
| Updates | A slow status RPC could overlap subsequent interval requests | Allow only one background status query at a time, releasing the guard after errors |
| Search | Each keystroke repeated workspace/category lookup and Unicode normalization per result | Build a dialog-scoped index; normalize the query once and replace results as a fragment |
| Keyboard | Up from the first result wrapped to the bottom instead of returning to search | Return focus to the field; support Home and End |
| Progress | Repeated labels rewrote live-region text and installation wording could imply closure too early | Preserve unchanged labels and describe closure as pending |

## Regression coverage

- Preferred-account startup failure leaves Overview available and attempts other accounts.
- Concurrent background status refresh calls issue a single RPC.
- Muted-account deletion permits saving the remaining notification preferences.
- Oversized state writes preserve the original file and leave no temporary artifact.
- Search remains usable across workspaces and returns keyboard focus to its field.
- Existing Rust, Python, Go updater and browser workflow suites remain required.

## Remaining acceptance work

- Measure CPU, memory and startup latency with real accounts on supported clean
  systems; these changes have no claimed percentage speedup.
- Validate long-running background sessions, screen-reader behavior with Orca,
  physical security keys, suspend/resume and disk-full conditions.
- Verify the installed AppImage upgrade and vault restoration after publication.
- Native dependencies still have the documented maintenance/advisory limitations
  in SECURITY.md. Synthetic tests do not remove those limitations.
