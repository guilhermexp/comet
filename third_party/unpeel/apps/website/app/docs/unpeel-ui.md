`unpeel-ui` is the Rust SDK Unpeel Apps are primarily built with. It sits on top of [Ratatui](https://ratatui.rs) — a wrapper, not a fork and not a second terminal renderer — and gives an App two things: the shared helpers that make it feel like part of the Unpeel family, and a portable view layer where **one view definition renders in any terminal today** and is ready for native renderers next.

The App contract stays protocols and files, never a language — any language that can draw in a terminal can be an App (see [Building an app](/docs/building-apps)). `unpeel-ui` is the golden path: it is what first-party Apps like `unpeel-todos` use, and it is the shortest route to an App that works everywhere Unpeel renders.

## The terminal-first helpers

Everything a plain Ratatui App needs to feel native to Unpeel, with zero `if unpeel` branches in your code:

- **`style`** — the shared style layer: colors, status styling, list/detail conventions, so Apps look like family.
- **`keys`** — the standard keybindings: j/k movement, the palette, help.
- **`fuzzy`** — fuzzy-scored matching for palettes and pickers.
- **`host`** — environment detection: one call tells you whether Unpeel is present.
- **`status`** — reports activity (busy, idle, needs-you) and a short status line ("3 open · 1 done") to the sidebar, the terminal UI, and your phone. Every call is a silent no-op when running standalone.

## One view, two render paths

Views built with the portable API are owned, serializable values that follow Ratatui's concepts and builder style:

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
```

The same `Node` tree renders two ways:

- **Terminal** — inside a normal Ratatui draw closure (`frame.render_widget(&view(...), area)`). The adapter lowers portable values to Ratatui's real widgets, layout solver, and backend — there is no second widget implementation.
- **Structured** — serialized over the `unpeel.ui/1` protocol: the App sends complete, revisioned snapshots of the tree as NDJSON and receives semantic action events back (stable node and action IDs, never raw key or pointer events). This is what lets a native or web renderer draw the same interface later while your Rust CLI keeps owning the model, validation, persistence, and business logic — no second backend in Swift or JavaScript.

An App checks `UNPEEL_UI_MODE=structured` to pick the path; a missing or unknown value always selects the terminal path. The terminal is the permanent fallback — structured rendering is additive, never required.

## What's portable today

Layout (Ratatui's constraint system: `Length`, `Percentage`, `Ratio`, `Min`/`Max`, `Fill`), styled text and paragraphs, tabs, lists, tables, a recording canvas with portable shapes, and generic **reorder** semantics: give ordered items stable IDs, declare `.reorderable("action")`, and every renderer — TUI grab/move/drop keys, native drag-and-drop, web pointer dragging — resolves to the same canonical event carrying the complete new ID order.

More built-in Ratatui concepts arrive through negotiated protocol versions without changing the terminal-first contract.

## The escape hatch

Raw Ratatui is re-exported as `unpeel_ui::ratatui`, pinned so your App compiles against the exact version the crate was built with. Any custom widget, `StatefulWidget`, or direct buffer drawing works in your terminal render branch, permanently. A custom widget has no automatic native representation — portability is explicit: an owned, serializable spec plus stable semantic actions.

## Getting started

`unpeel-todos` is the reference App and the fastest way to see the shape: a plain Ratatui program with a store, a handful of status calls, and a portable view. The crate's `examples/tabs_canvas.rs` is a complete dual-mode program, and the checked-in schema and fixtures under `protocol/` are the cross-language conformance source.

## Boundaries

`unpeel-ui` renders Unpeel App surfaces only. It never draws Unpeel's shell, sidebar, or terminal chrome, and — like every App surface — never code-editor UI: no diff views, file trees, or editor panes.
