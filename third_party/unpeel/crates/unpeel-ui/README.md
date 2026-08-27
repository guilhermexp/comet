# unpeel-ui

`unpeel-ui` is the Rust UI SDK for standalone-first Unpeel Apps.

It is a wrapper, not a Ratatui fork or a second terminal renderer. The
portable values retain widget meaning long enough to cross a process/language
boundary; the terminal adapter lowers them to Ratatui's real widgets, layout
solver, canvas rasterizer, buffer, and backend.

The SDK supports one view definition through two render paths:

- render it as a normal terminal UI through Ratatui;
- send the owned view tree through `unpeel.ui/1` so a future SwiftUI or web
  client can render the same interface and return semantic actions.

The protocol path and standalone example are built. Unpeel Host negotiation,
SwiftUI, and web integration are not built yet.

The target Mac experience can also compose a collapsible SwiftUI view above
the App's live Ghostty terminal. That hybrid form needs a dedicated Host UI
side channel so JSON never shares stdout with terminal bytes; it still uses
the same view tree and `unpeel.ui/1` messages. See the
[native rendering plan](../../docs/plans/unpeel-app-native-rendering.md) for
the end-to-end architecture and current implementation boundary.

The portable API follows Ratatui's concepts and builder style, but its values
are owned and serializable. Stable node and action IDs replace raw key and
pointer events at the protocol boundary. Protocol v1 sends complete,
revisioned snapshots; patches can be added later without changing the App
model loop.

```rust
use unpeel_ui::prelude::*;

fn view(selected: usize) -> Node {
    Tabs::new(["Overview", "Activity"])
        .id("main-tabs")
        .on_select("select-tab")
        .select(selected)
        .highlight_style(Style::new().fg(Color::Magenta).bold())
        .into()
}

// Standalone/TUI path, inside a normal Ratatui draw closure:
// frame.render_widget(&view(model.tab), frame.area());

// Structured path, with stdout reserved for NDJSON and logs on stderr:
// write_message(&mut stdout, &Snapshot::new(revision, view(model.tab)).into())?;
```

The stdio conversation is deliberately small:

```text
App  -> clientHello
Host -> hostHello
App  -> snapshot (revision 1, complete root tree)
Host -> event    (revision 1, nodeId + action + typed value)
App  -> snapshot (revision 2)
```

The Rust CLI stays running throughout this loop. It owns the App model,
validation, persistence, commands, and business logic. SwiftUI or web only
renders snapshots and returns actions; an App does not need a second backend
written in Swift or JavaScript.

The wire format omits many default-valued properties to keep full snapshots
small. Non-Rust decoders must apply the defaults declared by the v1 schema
(for example empty children/modifiers, zero padding/scroll, and default
styles) rather than requiring every property to be present. The checked-in
fixture corpus is the cross-language conformance input. Its NDJSON is a set of
independently valid frames, not one executable dialogue: the sample reorder
events are alternative actions against the same revision.

Portable layout values are logical Ratatui units, not pixels. They are exact
terminal cells in Ratatui; native/web renderers preserve relative layout and
interpret fixed horizontal/vertical units using their text and row metrics.

## Web layout

The same tree is designed to drive a future browser renderer without moving
the backend out of Rust.
`Layout::horizontal` and `Layout::vertical` become nested row/column
containers, and ordered constraints retain the Ratatui meaning:

| `unpeel-ui` value | Web interpretation |
|---|---|
| `Direction::Horizontal` / `Vertical` | flex or grid axis |
| `Length(n)` | `n` logical text-column or row units |
| `Percentage(n)` / `Ratio(a, b)` | share of the parent |
| `Min(n)` / `Max(n)` | logical minimum or maximum |
| `Fill(weight)` | weighted remaining space |
| `Flex` and `Spacing` | alignment/distribution and gap |

Layouts can nest to form responsive interfaces. A web renderer may adapt
simple cases to CSS. Exact Ratatui geometry would require reusing its ordered
Cassowary behavior (for example through shared/Wasm code) and checking it
against golden cases; CSS flex alone is not identical for every mixed or
unsatisfiable constraint set. The promise is semantic and responsive parity,
not identical terminal-cell pixels. The browser receives versioned snapshots
and returns the same action events as SwiftUI; App state, persistence,
validation, and commands stay in the running Rust CLI. The web renderer and
Host transport are planned; the portable layout contract is already
implemented. See
[Ratatui's layout concepts](https://ratatui.rs/concepts/layout/) for the
source semantics that the renderers preserve.

## Stateful widgets and the compatibility boundary

Raw Ratatui remains the complete terminal escape hatch, so any
`StatefulWidget` can still be used in an App's separate raw/TUI render branch.
It cannot be inserted into a portable `Node` automatically. For structured
mode, the App must author a portable equivalent or fallback. Native/web
portability is explicit: a widget needs an owned, serializable spec plus
stable semantic actions. Ratatui's trait only requires an associated `State`
and `render(area, buffer, &mut state)`; rendering its cells does not reveal
enough meaning to synthesize an accessible SwiftUI or web control.

Portable `List` and `Table` snapshots include selection and scroll offset.
Each portable render seeds a fresh real Ratatui `ListState`/`TableState` from
those values. Ratatui-computed offset mutations are discarded and not exposed;
an App that needs cross-draw offset continuity must own and update that offset
itself. The separate raw Ratatui branch retains normal state objects and
therefore supports the full cross-draw `StatefulWidget` behavior. More
built-ins, such as a scrollbar, can gain equivalent portable adapters over
time. A custom or not-yet-adapted stateful widget has no native/web
representation. See Ratatui's
[`StatefulWidget` contract](https://docs.rs/ratatui/latest/ratatui/widgets/trait.StatefulWidget.html).

## Reorder cards, lists, tabs, and tables

Reordering is shared behavior rather than a platform-specific drag event.
Every ordered item gets a stable ID, and `.reorderable(...)` declares the
semantic action. Cards are ordinary keyed children of a `Layout`:

```rust
use unpeel_ui::prelude::*;

let cards: Node = Layout::vertical([
    Constraint::Length(3),
    Constraint::Length(3),
])
.id("task-board")
.children([
    Node::from(Paragraph::new("Write docs")).id("task-docs"),
    Node::from(Paragraph::new("Ship app")).id("task-ship"),
])
.reorderable("reorder-tasks")
.into();

let list: Node = List::new([
    ListItem::new("Write docs").id("task-docs"),
    ListItem::new("Ship app").id("task-ship"),
])
.id("task-list")
.reorderable("reorder-tasks")
.into();

let table: Node = Table::new(
    [
        Row::new(["Write docs", "Ready"]).id("task-docs"),
        Row::new(["Ship app", "Blocked"]).id("task-ship"),
    ],
    [Constraint::Percentage(70), Constraint::Percentage(30)],
)
.id("task-table")
.column_ids(["title", "status"])
.reorderable_rows("reorder-rows")
.reorderable_columns("reorder-columns")
.into();
```

SwiftUI can use drag/drop and accessibility move actions; web can use pointer
dragging plus keyboard controls; a TUI can use grab/move/drop keys and
optional mouse reporting. They resolve to the same complete stable-ID order.
Structured renderers encode that order as the canonical event below; the TUI
helper returns it in `ReorderUpdate::Committed` for the local reducer:

```json
{
  "type": "event",
  "protocol": "unpeel.ui",
  "protocolVersion": 1,
  "revision": 7,
  "nodeId": "task-table",
  "action": "reorder-rows",
  "kind": "change",
  "value": { "type": "textList", "value": ["task-ship", "task-docs"] }
}
```

The value is the complete logical order of IDs. The App reducer matches the
revision, node, and action, then calls `apply_order`, persists the accepted
model, and publishes the next snapshot. `ReorderState` supplies the
renderer-neutral TUI grab/move/drop state machine and `ActionEvent::reorder`
builds the wire event. Table row and column actions are deliberately
separate. Manual reorder is also distinct from sorting by a data field.

The terminal and structured reducers use the same model vector:

```rust
use unpeel_ui::{keys, prelude::*};

struct Task {
    id: ItemId,
    // domain fields...
}

struct Model {
    tasks: Vec<Task>,
    selected: Option<usize>,
    reorder: ReorderState,
}

// TUI: map List keys with keys::reorder_list(key, direction), or a mouse
// hit-test with ReorderCommand::MoveTo(index).
let command = keys::reorder_list(&key, ListDirection::TopToBottom)
    .expect("handle only mapped keys here");
let update = model.reorder.handle(command, &mut model.tasks, |task| {
    Some(task.id.as_str())
})?;
if let ReorderUpdate::Committed { order } = update {
    persist_order(&order)?;
}

// SwiftUI/web event: first match the current revision, nodeId, and action.
if let Some(order) = event.reorder_ids() {
    let applied = apply_order(&mut model.tasks, &order, |task| Some(task.id.as_str()))?;
    model.selected = applied.remap_index(model.selected); // same task, new index
    persist_order(&order)?;
}
```

Set `UNPEEL_UI_MODE=structured` when the Host has reserved stdin/stdout for
that protocol. A missing or unknown value always selects the terminal path.
See `examples/tabs_canvas.rs` for a complete dual-mode program and the
canonical schema/fixtures under the repository's `protocol/` directory.

This package provides the owned model, validation, JSON framing, generic
reorder helper, and real-Ratatui adapter. Host structured-session plumbing and
the SwiftUI/web renderers are separate follow-up work. Terminal input remains
the App's normal Ratatui event loop: action metadata does not replace terminal
key handling.

Raw Ratatui remains available as `unpeel_ui::ratatui`. It is the permanent
terminal fallback for custom widgets, direct buffer drawing, or anything a
native renderer does not understand. Existing `style`, `keys`, `fuzzy`,
`host`, and `status` helpers remain source-compatible.

The portable foundation currently includes layout, styled text/paragraphs,
tabs, lists, tables, generic reorder semantics, and a recording canvas with
portable shapes. More built-in Ratatui concepts will be added through
negotiated protocol versions/capabilities without changing the terminal-first
contract.

This SDK is for Unpeel App surfaces only. It does not render the Unpeel shell
or add code-editor UI.
