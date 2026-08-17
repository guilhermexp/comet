# Unpeel Workers menu-bar parity design

## Status

Approved by the user on 2026-08-17. The pinned Unpeel source at
`third_party/unpeel` is the authority. This is a deterministic port, not a
reinterpretation of the supplied screenshots.

## Goal

Add the pinned Unpeel macOS menu-bar activity surface to Comet's **Workers**
mode. It remains present while the process is running, including after the
main window is closed, reflects live Workers activity, and reopens the main
window to the selected Worker session.

Orchestrator sessions do not participate in this stage.

## Source map

The port must stay aligned with these pinned files:

- `apps/native/UnpeelNative/Sources/UnpeelNative/MenuBarController.swift`:
  `NSStatusItem`, `NSPopover`, button state machine, animation timer, explicit
  popover sizing, selection callbacks, and window resurrection.
- `apps/native/UnpeelNative/Sources/UnpeelNative/Views/RootView.swift`:
  `ActivityMenuSessions`, `ActivityMenuList`, `AllRecentMenuRow`,
  `TitlebarActivityMenuRow`, and braille spinner presentation.
- `apps/native/UnpeelNative/Sources/UnpeelNative/UnpeelStore.swift`:
  `activeJobSessions`, `unreadJobSessions`, `activityProjectName`,
  `activityStatusLabel`, `revealSessionInSidebar`, and `openRecentActivity`.
- `apps/native/UnpeelNative/Sources/UnpeelNative/Views/RecentActivityView.swift`:
  the destination behind **All recent**.
- `apps/native/UnpeelNative/Sources/UnpeelNative/AppDelegate.swift`:
  application lifetime with no main window, rebuild-on-demand, and the
  one-runloop-turn delay before revealing a session in a rebuilt sidebar.
- `apps/native/UnpeelNative/Sources/UnpeelNative/Views/TerminalArea.swift`:
  template idle mark and badged attention/unread mark construction.

The submodule stays unmodified.

## Exact behavior

### Application lifetime

`WorkersModel` moves from `Shell` ownership to application ownership. Its
snapshot polling and activity bridge therefore continue after Command-W closes
the last window. Reopening from the Dock or status item rebuilds `Shell` around
the same model; it never creates a second Workers store or resets selection.

Explicit application quit remains the only process teardown.

### Status item state machine

The status item is always present on macOS and has four mutually exclusive
modes, in the same precedence as Unpeel:

1. `working(blocked)` when any Worker is starting, working, or restarting;
2. `blocked` when none is working and at least one Worker needs attention;
3. `unread` when none is working/blocked and at least one settled Worker is
   unread;
4. `idle` otherwise.

Working cycles the existing ten braille frames every 120 ms. If a blocker
coexists with working sessions, the spinner uses the attention tint. Blocked
uses the amber badge, unread uses the blue badge, and idle uses the template
application mark. The frame timer exists only while the visible mode is
working; snapshot publications drive all other refreshes.

### Activity projection

The popup projection is deterministic and de-duplicated by session id:

- blockers first, preserving visible project-tree order;
- working/starting/restarting sessions second, sorted by recent activity;
- settled unread sessions last, excluding ids already shown above.

Each row uses the prompt-derived title with `Untitled session` fallback,
project display name, provider SVG, provider spinner tint, and one of the exact
visual forms:

- working: colored braille spinner leading and provider icon trailing;
- blocked: amber dot leading and `Blocked` trailing;
- unread: blue dot leading and provider icon trailing.

Dividers appear only between non-empty sections. With no rows, the popup shows
`No active sessions`. The `All recent` footer is always present.

### Geometry and interaction

- `NSStatusItem.variableLength` anchored to a transient animated `NSPopover`.
- Popover content width: 320 pt, with 6 pt outer padding.
- Explicit popover width: 332 pt.
- Activity rows: approximately 42 pt including two text lines and vertical
  padding; internal stack spacing is 2 pt.
- Footer: 28 pt.
- Empty body: 34 pt plus 12 pt outer vertical padding.
- Dividers add 9 pt between populated sections.
- Row horizontal padding is 8 pt, vertical padding is 6 pt, and hover radius is
  7 pt.

The popover activates the app before showing, inherits the app appearance,
becomes key, closes transiently, and anchors to the visual bottom of the status
button. Clicking an activity row closes the popup, reopens/focuses the main
window, switches to Workers, and selects the exact session after the rebuilt
shell has mounted.

### All recent

`All recent` closes the popup, reopens/focuses Comet, switches to Workers, and
opens a Workers recent-activity route. The page follows the pinned source:

- active sessions first;
- persisted/reconstructed recent activity newest first and grouped as Today,
  Yesterday, or localized date;
- live rows navigate to their session;
- removed sessions remain visible but disabled/muted when historical data is
  available;
- empty copy is `No recent activity` plus the upstream explanatory sentence.

If the current adapter does not yet expose the upstream persisted activity log,
that missing canonical field must be added at the adapter boundary rather than
inventing a UI-local history schema.

## Technical boundary

The macOS shell is implemented in-process with AppKit through the existing
Objective-C runtime dependency. It uses native `NSStatusItem` and `NSPopover`,
not an `NSMenu`, helper process, Swift sidecar, or simulated GPUI floating
window. Non-macOS builds receive a no-op adapter and retain their current
behavior.

The native controller consumes an immutable presentation snapshot derived from
`WorkersModel`; it does not read Unpeel state directly. Native callbacks are
reduced to typed intents (`SelectSession`, `ShowAllRecent`) and delivered back
to the GPUI main thread. This preserves one owner for selection, unread clearing,
routes, polling, and errors.

## Failure behavior

- Failure to create the AppKit item logs once and leaves the main app usable.
- An activity refresh never blocks the AppKit main thread on disk or host I/O.
- A clicked session that disappeared before dispatch closes the popup and
  reopens Workers without selecting a different session.
- Closing/reopening the window never duplicates polling, hook servers, status
  items, or spinner timers.
- Explicit quit releases the status item and follows the existing Workers and
  engine shutdown path.

## Acceptance gates

- Reducer tests prove the exact four-mode precedence and section de-duplication.
- TDD covers persistent model ownership across window reconstruction, exact
  row selection intent, `All recent`, and a disappearing session.
- macOS adapter tests cover explicit size calculations and native intent tags
  without requiring a visible desktop.
- Existing Workers adapter/model/presentation suites remain green.
- `cargo fmt --all -- --check`, `cargo test -p zeron-ui workers`,
  `cargo test -p zeron-workers-unpeel`, and `cargo build -p zeron` pass.
- A real bundled dev build is checked with the main window visible, hidden,
  closed, reopened from the menu bar, working, blocked, unread, empty, and
  multiple-project Workers states.
- Visual comparison uses the pinned Unpeel app/source and the supplied menu-bar
  screenshots; a passing unit suite alone is not completion.
