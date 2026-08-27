# Native Views Above the Terminal for Unpeel Apps

> **Status (amended 2026-08-15): architecture preview; expanded foundation built,
> integration not built.** `unpeel-ui` provides the portable Rust view model,
> real Ratatui renderer, `unpeel.ui/1` messages, schema, fixtures, validation,
> layout/paragraph/tabs/list/table/canvas nodes, stable reorder metadata, and
> the shared reorder state/apply helper. The Host side channel, Swift
> decoder/renderer, stacked native view, durable replay, and remote/web
> renderers remain planned. Before public SDK release the `unpeel-ui`
> crate/package is renamed to the **Unpeel Apps UI SDK**,
> `unpeel-apps-ui-sdk`; the `unpeel.ui/1` wire name remains stable.

## The product promise

The target keeps an Unpeel App as a normal Rust CLI.

- Run the binary in a regular terminal and it renders a real Ratatui TUI.
- Run the same binary inside the Unpeel Mac app and Unpeel can render its
  portable view as SwiftUI above the live terminal.
- Collapse the native view and the terminal is still there. If native
  rendering is unavailable, the App continues to work as a TUI.

There is one App process and one App model. Swift does not reimplement the
App's storage, commands, validation, or business logic. It renders the view
described by Rust and returns semantic actions to Rust.

```text
┌──────────────────── Unpeel session ────────────────────┐
│ Native Unpeel App view (SwiftUI, collapsible/resizable) │
├──────────────────────── divider ────────────────────────┤
│ Live terminal (Ghostty, the App's real hosted PTY)      │
└─────────────────────────────────────────────────────────┘
```

This stacked layout is a client-owned presentation of one App **surface**. It
is not a third App target and it is not the companion **panel** beside another
session.

## One Rust process, two render paths

The App builds an owned `unpeel-ui` tree from its model. That tree can be
lowered to Ratatui or serialized without changing the model:

```rust
use unpeel_ui::prelude::*;

fn view(selected: usize) -> Node {
    Tabs::new(["Overview", "Activity"])
        .id("main-tabs")
        .on_select("select-tab")
        .select(selected)
        .into()
}

// Terminal path:
// frame.render_widget(&view(model.selected), frame.area());

// Native path:
// publish Snapshot::new(revision, view(model.selected));
```

In the planned hybrid Mac presentation, both paths belong to the same
process:

```text
                              ┌─ portable snapshot ─▶ Host ─▶ SwiftUI
Rust Unpeel App ─ UI bridge ──┤                         │
       │                      └◀ semantic event ────────┘
       │
       └─ stdin/stdout PTY ◀▶ Host ◀▶ Ghostty terminal
```

Terminal keys and native actions enter the same Rust event loop. The App
applies either input to its model and publishes the next revision. There is no
Swift shadow model that can drift from the Rust backend.

## Why the native protocol needs a side channel

The existing `unpeel-ui` example demonstrates **structured-only mode**:
`UNPEEL_UI_MODE=structured` reserves stdin/stdout for newline-delimited JSON.
That is suitable when the native surface replaces the terminal presentation,
but JSON protocol frames and ANSI terminal bytes cannot safely share stdout.

The stacked native-plus-terminal presentation therefore requires a dedicated,
session-scoped Host channel—an inherited file descriptor or Unix socket—while
stdin/stdout remain attached to the PTY. The exact local channel is an
implementation detail; its payload is still the same bounded, versioned
`unpeel.ui/1` protocol. This is a second transport for the same UI contract,
not a second set of UI verbs.

This gives Unpeel four useful presentations:

| Environment | Presentation | Transport |
|---|---|---|
| Bare terminal | Ratatui TUI | process PTY |
| Unpeel, terminal-only | Ratatui in Ghostty | hosted PTY |
| Unpeel Mac, hybrid | SwiftUI above Ratatui/Ghostty | UI side channel + hosted PTY |
| Unpeel, structured-only | native App surface | JSON stdio; terminal requires relaunch/fallback |

The hybrid path is the intended Mac experience for an App that supports both
views. Structured-only mode remains useful for headless/native-only clients
and as the current protocol example.

## Snapshot and action loop

The side channel uses the same handshake as structured stdio:

```text
App  -> clientHello
Host -> hostHello
App  -> snapshot (revision 1, complete root tree)
Host -> event    (revision 1, nodeId + action + typed value)
App  -> snapshot (revision 2)
```

Every interactive node has a stable `nodeId` and stable action name. SwiftUI
returns intent such as `select`, `activate`, `change`, `submit`, or `cancel`;
it does not send raw key codes or pointer coordinates. The App reducer
validates the event against the snapshot revision, updates its state, and
emits a new full snapshot.

Protocol v1 uses complete snapshots deliberately. Patches are a later
optimization, not a second state model.

## Shared interaction helpers: reorderable collections

Drag-and-drop should be modeled as the semantic behavior **reorder**, because
a terminal, SwiftUI, web, keyboard, touch, and assistive technology do not
share one physical drag gesture.

Reorder is generic behavior exposed by the current portable `Layout`, `Tabs`,
`List`, and `Table` nodes. Future ordered widgets can adopt the same stable-ID
contract:

| Collection | Stable identity | Result |
|---|---|---|
| Cards, lists, and tabs | item or tab IDs | new item-ID order |
| Table rows | row IDs | new row-ID order |
| Table columns | column IDs | new column-ID order |
| Kanban columns or sibling groups | child IDs within a parent | new sibling order |

The implemented API keeps the common behavior small while giving tables
explicit row and column entry points. Cards are keyed child nodes in a
reorderable layout; they do not need a second container vocabulary:

```rust
let cards = Layout::vertical(tasks.iter().map(|_| Constraint::Length(3)))
.id("task-board")
.children(tasks.iter().map(|task| {
    Node::from(Paragraph::new(task.title.clone())).id(task.id.clone())
}))
.reorderable("reorder-tasks");

let list = List::new(tasks.iter().map(|task| {
    ListItem::new(task.title.clone()).id(task.id.clone())
}))
.id("task-list")
.reorderable("reorder-tasks");

let table = Table::new(rows, widths)
    .id("task-table")
    .row_ids(tasks.iter().map(|task| task.id.clone()))
    .reorderable_rows("reorder-rows")
    .column_ids(["title", "owner", "status"])
    .reorderable_columns("reorder-columns");
```

Each planned presentation maps its native input to the same stable-ID order:

- **Ratatui:** Space/Enter picks up and drops the focused item; arrows move it
  along the collection's axis. A focused table header reorders columns with
  Left/Right, while a focused row uses Up/Down. Mouse drag can be an additional
  shortcut when terminal mouse reporting is active. The App supplies focus
  routing and mouse hit-testing; `unpeel-ui` maps axis keys and accepts a
  `MoveTo(index)` computed by that external hit test.
- **SwiftUI:** native drag preview, drop target, animation, keyboard move
  commands, and VoiceOver move actions.
- **Web:** pointer/HTML drag-and-drop plus keyboard-accessible move controls.

The implemented wire form uses the existing `change` event with `textList`:
the value is the complete ordered list of stable IDs for the declared scope.
The node and action distinguish table rows from columns. A later additive
value may encode `move item_id before anchor_id` if large collections make
that useful.

```json
{
  "type": "event",
  "protocol": "unpeel.ui",
  "protocolVersion": 1,
  "revision": 7,
  "nodeId": "task-board",
  "action": "reorder-tasks",
  "kind": "change",
  "value": {
    "type": "textList",
    "value": ["card-c", "card-a", "card-b"]
  }
}
```

The renderer may animate an optimistic order, but the App reducer must match
the revision/node/action, use `apply_order` to validate the IDs, persist the
accepted model, and confirm it in the next snapshot. The Host/renderer must
reject or ignore a stale reorder and restore the latest authoritative
snapshot; clients never silently commit their own copy of App state. Raw drag
coordinates never cross the protocol.

Manual reorder and data sorting are different actions. "Sort rows by status"
asks Rust to derive an order from a field; "move this row above that row"
changes a user-owned order. A table may expose both, but renderers and Apps
must never infer one from the other. Moving an item between two collections is
likewise a separate future **transfer** behavior rather than an ambiguous
reorder.

`unpeel-ui` now provides two halves of the helper:

1. portable reorder metadata represented in snapshots for current and future
   renderers;
2. `ReorderState`, `ReorderCommand`, axis-specific key mappings,
   index-targeted `MoveTo`, `ActionEvent::reorder`, and exact-permutation
   `apply_order`.

That keeps the App's reducer identical whether a person drags a Swift card,
moves a table column with VoiceOver, or moves a terminal row with the
keyboard.

## Stateful widgets and web layout

The compatibility rule is explicit. Any raw Ratatui `Widget` or
`StatefulWidget` remains usable in the terminal fallback, but SwiftUI/web can
only render owned portable specs whose state, identity, content, and actions
are represented in the protocol. The current portable `List` and `Table`
carry selection and offset state and lower through Ratatui's real
`ListState`/`TableState`. The portable renderer seeds temporary state on each
draw; Ratatui-computed offsets are discarded and not exposed. An App that
needs cross-draw continuity must own and update its offset independently. A
custom stateful widget, or a built-in such as `Scrollbar` before an adapter
exists, remains terminal-only.

Web uses the same recursive `Layout` tree. Horizontal/vertical direction maps
to a row/column container; Percentage/Ratio/Fill map to relative allocation;
Min/Max remain bounds; Length uses logical text-column or row metrics; Flex
and Spacing map to distribution and gaps. Simple layouts can adapt to CSS
flex/grid. Exact Ratatui geometry would require reusing its ordered Cassowary
behavior and golden-testing it; CSS alone is not equivalent for every mixed
or unsatisfiable constraint set. The result targets responsive semantic
parity rather than terminal-pixel identity.

## Who owns what

| Component | Responsibility |
|---|---|
| Rust Unpeel App | Model, persistence, domain rules, validation, commands, view construction, action handling |
| `unpeel-host` | Process/PTY lifetime, session-scoped UI channel, frame limits, latest valid snapshot, controller routing |
| Swift app | Decode validated nodes, render accessible native controls, choose layout, return semantic actions |
| Ghostty | Render the App's real terminal output and accept normal terminal input |
| Unpeel-operated cloud | Nothing; App UI and state remain on the user's Host |

The native renderer may adapt presentation to the platform. Ratatui
`Length(3)` means three cells in a terminal; SwiftUI interprets it using text
and row metrics and may relax it for accessibility. The native view preserves
meaning, order, selection, actions, and content—not terminal-cell pixels.

## Native composition on macOS

Conceptually, `TerminalArea` gains a native App region above the existing
`GhosttyTerminalPane`:

```swift
// Directional pseudocode; these types do not exist yet.
VStack(spacing: 0) {
    if let snapshot = session.appUISnapshot, !session.nativeViewCollapsed {
        UnpeelAppView(root: snapshot.root) { action in
            host.sendAppUIAction(action, to: session.id)
        }
        .frame(minHeight: 120)

        Divider()
    }

    GhosttyTerminalView(session: session)
}
```

Presentation rules:

- The native region is optional, collapsible, and user-resizable.
- Hiding it never stops or restarts the Rust process.
- Clicking a native control sends a semantic action; focusing the terminal
  continues to send ordinary PTY input.
- Native state is derived from the latest validated snapshot. Swift-only
  state is limited to presentation details such as collapse state, split
  height, and in-progress drag animation.
- This renders only the Unpeel App surface. It cannot generate the Unpeel
  sidebar, session chrome, file trees, diff viewers, or editor panes.

## Lifecycle and survival

The target lifecycle is:

1. The Host launches the App in its normal hosted PTY and offers the optional
   session-scoped UI channel.
2. An App that supports hybrid rendering keeps its Ratatui event loop running
   and sends `clientHello` on the UI channel.
3. After `hostHello`, the App publishes its first snapshot. The Swift app
   mounts the native region only after that snapshot validates.
4. Native actions travel through the Host to the same Rust process. The Host
   rejects invalid identifiers, oversized frames, and the wrong protocol
   version at the boundary; Rust rejects actions against stale revisions.
5. The Host retains the latest valid snapshot so a Swift controller can
   disappear and reconnect without becoming the App's authority. Durable
   replay and remote transport use the shared Host session contract.
6. When the App exits, both its terminal and native region close as one
   session.

The Host must advertise this capability. Controllers never guess support from
the platform, Host kind, or a failed route probe.

## Fallback is part of the design

The terminal is the compatibility layer, not a debugging afterthought:

- No Host UI channel: run the TUI normally.
- No `clientHello` or first valid snapshot: keep the native region hidden.
- Channel disconnect or malformed/oversized frame: collapse the native region
  and leave the PTY alive.
- Unsupported protocol version, node, or option: show a concise compatibility
  notice and use the terminal rather than guessing native behavior.
- Custom Ratatui widgets, direct `Buffer` drawing, and non-portable paint code:
  render in the terminal only.
- A stale native action: ignore it; never apply it to a newer Rust model.

Because hybrid mode keeps the PTY alive, most failures require no relaunch.
Structured-only sessions still need the planned stop-and-relaunch-in-terminal
fallback.

## Current implementation boundary

Built now:

- owned `Layout`, styled text/`Paragraph`, `Tabs`, `List`, `Table`, and
  recording `Canvas` specs in `unpeel-ui`;
- lowering through Ratatui's real layout solver, widgets, canvas rasterizer,
  buffer, and backend;
- bounded `unpeel.ui/1` NDJSON framing and typed semantic events;
- stable IDs on layout children/cards, tabs, list items, table rows, and table
  columns, with reorder actions owned by their collection nodes;
- generic grab/move/drop state, keyboard mappings, index-targeted movement for
  externally hit-tested input, complete-order events, exact-permutation
  application, cancel restoration, and selection preservation by identity;
- schema, exhaustive wire fixtures, validation, and a dual-mode example;
- permanent access to raw Ratatui for terminal-only behavior.

Still to build:

- the Host's hybrid UI side channel and latest-snapshot cache;
- Host capability advertisement and controller routing for App UI;
- generated or hand-maintained Swift protocol types checked against the shared
  fixtures;
- SwiftUI renderers for the current portable nodes;
- the collapsible/resizable native region in the Mac session area;
- lifecycle, reconnect, fallback, reorder, and cross-renderer conformance
  tests;
- iPhone/iPad and future web renderers over the same Host contract;
- broader built-in Ratatui coverage.

## Implementation order

1. Add the session-scoped auxiliary UI channel without changing PTY behavior.
2. Cache and expose the latest validated snapshot through the shared Host
   adapter; advertise the capability.
3. Decode the existing schema/fixtures in Swift and render the current layout,
   paragraph, tabs, list, table, and canvas slice.
4. Compose the native region above `GhosttyTerminalPane`, with collapse and
   resize state owned by the Swift client.
5. Map SwiftUI drag/drop, keyboard move commands, and accessibility actions
   onto the implemented reorder contract; wire optional TUI mouse hit-testing
   to `ReorderCommand::MoveTo`.
6. Route all revisioned semantic actions back to Rust and prove reconnect and
   failure fallback without killing the PTY.
7. Run the same fixture/golden corpus through Ratatui and SwiftUI in CI.
8. Extend the shared Host route to remote controllers, then add iOS/web
   renderers without creating App-specific networking.

This remains sequenced behind the Apps SDK and Host work in
`master-plan-next.md`; this document defines the target composition, not a
claim that it ships today.

## Main files and contracts

- `crates/unpeel-ui/README.md` — current Rust SDK usage and boundaries.
- `crates/unpeel-ui/examples/tabs_canvas.rs` — working terminal/structured
  example.
- `crates/unpeel-ui/src/portable/` — owned nodes, protocol, validation, and
  Ratatui adapter.
- `protocol/unpeel-ui-v1.schema.json` — canonical v1 wire schema.
- `protocol/unpeel-ui-fixtures-v1.json` — cross-language fixture corpus.
- `apps/native/UnpeelNative/Sources/UnpeelNative/Views/TerminalArea.swift` —
  future native composition point.
- `docs/plans/unpeel-apps.md` — authoritative App/runtime/data contract.
- `docs/plans/unpeel-plugins.md` — detailed Horizon A/B implementation plan.
- `docs/plans/dual-mode-sessions.md` — structured hosted-session machinery
  shared with native agent/chat sessions.
