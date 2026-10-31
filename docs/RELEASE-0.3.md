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

## WhatsApp presentation integration

The channel header now includes the cached own-profile photo (or initials), service
label and channel settings. The Discord-style workspace/category/channel layout
and WhatsApp conversation layout stay intact. Loaded account switches reveal the
destination before hiding siblings; an account still loading may show WhatsApp's
own loader. No new readiness or connection-health claim is made.

A top-frame WebKit user stylesheet is restricted to `https://web.whatsapp.com/*`.
It aligns known dark-theme surface variables and the conversation-list background.
Light or unrecognized themes retain their original presentation. It does not inspect
messages, override message colors, alter dimensions, hide functional controls,
relax permissions or request remote resources. WhatsApp may change these selectors;
real-account visual validation is still needed. Promotional elements are left alone
until an exact, safe target can be verified.

## Everyday workflow refinements

- Header recovery actions appear for load, TLS or process errors, with a dedicated
  reconnect label after process exit. QR readiness points to Linked devices; this
  still does not claim to monitor WhatsApp transport health. Manual reload remains
  in channel settings even when the page is healthy.
- Drag local channel rows onto workspace icons, category sections or the workspace
  heading (uncategorized). External drag payloads are ignored. The latest move can
  be undone in the sidebar; a failed move is not shown as successful. Existing
  placement selectors and move-up/down actions remain the keyboard alternative.
- Channel settings can mute/unmute notifications; the bell-with-slash indicator
  marks muted channels while unread counters remain visible.
- Update progress shows current/target versions, actual downloaded bytes and
  separate signature/download verification and installation stages. The helper
  atomically publishes size-only progress in the private runtime directory; no
  paths, messages or URLs are sent to the interface. Percentages are shown only
  for a known download size, never fabricated for cryptographic verification.
- General settings provide Comfortable/Compact density and an animation toggle,
  saved automatically in local shell storage. No account data is stored in these
  preferences. System reduced motion takes priority. Density never changes the
  native account viewport, header height or sidebar width.
- Ctrl+, opens Settings and Ctrl+Shift+L locks the vault from the local interface;
  Ctrl+K remains the quick switcher. Remote WhatsApp focus retains its own shortcuts.

## Workspace continuity and prepared updates

- The unlocked shell remembers workspace/channel selection (including Overview),
  collapsed categories, favorites and channel filter in its encrypted profile.
  Invalid/deleted selections fall back to an existing workspace/channel. Window
  size and maximized state are restored from the respective UI profile, with safe
  minimum/maximum bounds and debounced writes; window position is not restored.
- All/Favorites/Unread filters retain category organization. Unread uses the
  existing WhatsApp counter or notification fallback, not a new message scanner.
- Right-click channel/workspace menus support arrow keys, Home/End, Enter and
  Escape. Channel placement remains available through the existing dialog.
- Release checks now prepare and verify the public AppImage in the background.
  A ready artifact is retained and waits for explicit installation during use;
  startup still installs before unlock. Network cooldowns and signature checks
  remain enforced. Manual checks cannot discard a prepared update.
- Text size choices (100/110/120%) scale local reading text without changing the
  native viewport geometry. WhatsApp retains its own text controls. Existing
  reduced-motion support and higher-contrast system preference remain respected.
