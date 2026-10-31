# 0.3.0 — Design and everyday workflows

Published directly as a stable release. Run 37 starts 0.3.0; later pushes increment
the patch number. No release-candidate channel or automatic version commits.

## Scope

- A cohesive graphite, silver and soft-violet visual system, with clearer type,
  quieter surfaces, consistent spacing and a prominent Connect action.
- Add WhatsApp directly from the workspace header. Channel location uses an
  expandable section and defaults to the current workspace; category destinations
  remain visible when relevant. QR linking follows channel creation as before.
- Quick switch searches channels and workspaces and offers Add WhatsApp, Create
  workspace and Settings without browsing multiple menus. Arrow keys, Enter and
  Escape remain available. Search stays local and uses no conversation data.
- Short, bounded entry and interaction animations on local cards and dialogs.
  Reduced-motion preference disables them. No new dependencies, external fonts,
  animation loops, remote-page effects or geometry changes to native accounts.
- Existing vault, permissions, notification, cache and update protections remain.

## Validation

Browser regressions cover account creation, placement, quick switching and settings;
visual review covers 900, 1200 and 1600px windows. Native frame and account geometry
are unchanged. Real desktop acceptance and clean-system certification remain listed
in VALIDATION.md; a stable label does not claim those checks are complete.

## Next series

0.4 is reserved for a separately scoped security review and improvements. No new
security guarantees are introduced by this visual release.
