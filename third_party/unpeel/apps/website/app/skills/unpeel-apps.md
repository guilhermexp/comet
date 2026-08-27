---
name: unpeel-apps
description: Build an Unpeel App — a standalone-first terminal CLI that lights up inside Unpeel with sidebar status, activity, and phone streaming. Use when asked to create an Unpeel App, integrate a CLI tool with Unpeel, or build with the unpeel-ui Rust crate.
---

# Building an Unpeel App

An Unpeel App is a standalone-first terminal program — a task list, notes tool, dashboard, ops tool — that Unpeel launches, persists, streams, and remotes exactly like an agent session. Full docs: https://unpeel.com/docs/unpeel-apps and https://unpeel.com/docs/building-apps.

## Hard rules (violating any of these disqualifies the app)

1. **Standalone first.** The binary must install, run, and be fully useful in a bare terminal with no Unpeel present. Never gate core function on Unpeel; never require Unpeel-only setup before first run.
2. **Name the binary `unpeel-<name>`.** Unpeel's startup PATH scan recognizes the prefix and seeds a launch preset automatically — that is the entire distribution story. No store, no registration, no manifest server.
3. **No IDE chrome.** No diff views, file trees, editor panes, or symbol navigation — Unpeel is not a code IDE and refuses that surface everywhere.
4. **No Node runtime.** First-party integrations never ship or require Node.
5. **No daemons.** An App instance is a hosted session owned by Unpeel's host process; never fork a background service of your own.

## The golden path: Rust + `unpeel-ui`

`unpeel-ui` wraps [Ratatui](https://ratatui.rs) — a wrapper, not a fork; raw Ratatui stays available as `unpeel_ui::ratatui` (pinned re-export, use it instead of a direct ratatui dependency). Docs: https://unpeel.com/docs/unpeel-ui.

Structure the app as a normal Ratatui program: a model/store, an event loop, a draw closure. Then use the SDK helpers:

- `unpeel_ui::style` — shared colors, status styling, list/detail conventions. Use these instead of hardcoding a palette.
- `unpeel_ui::keys` — standard keybindings: j/k movement, the palette, help. Match them so the app feels like family.
- `unpeel_ui::fuzzy` — fuzzy-scored matching for palettes and pickers.
- `unpeel_ui::host` — environment detection; one call answers "is Unpeel here?" (presence of `UNPEEL_SESSION_ID` and friends).
- `unpeel_ui::status` — report activity and a status line to the sidebar.

## Status reporting (what makes the sidebar live)

Report three states plus one line of text; every call is a silent no-op when running standalone, so write zero `if unpeel` branches:

- **busy** while doing work, **idle** when done, **attention** when the app needs the user;
- a short status line, e.g. `"3 open · 1 done"` — it renders under the app's sidebar row on desktop, in the terminal UI, and on the phone.

Report transitions when they happen, not on a timer. Unpeel's activity engine, unread badges, and phone notifications then work unmodified.

## Portable views (optional, recommended)

Build views with `unpeel_ui::prelude` portable values (owned, serializable, Ratatui-style builders with stable `.id(...)` node ids and semantic `.on_*("action")` ids):

```rust
use unpeel_ui::prelude::*;

fn view(selected: usize) -> Node {
    Tabs::new(["Overview", "Activity"])
        .id("main-tabs")
        .on_select("select-tab")
        .select(selected)
        .into()
}
```

One tree renders two ways: pass it to `frame.render_widget(...)` in the terminal branch, or — when `UNPEEL_UI_MODE=structured` — write complete revisioned snapshots as NDJSON on stdout and read semantic action events back (protocol `unpeel.ui/1`). A missing or unknown mode value always selects the terminal path; the terminal is the permanent fallback. In structured mode stdout belongs to the protocol — logs go to stderr.

Portable today: layout constraints, styled text/paragraphs, tabs, lists, tables, a recording canvas, and reorder (`.reorderable("action")` on stable item ids; the event carries the complete new id order). Custom/stateful widgets stay in the raw Ratatui terminal branch.

## Reference

- `unpeel-todos` is the reference App: a plain Ratatui program with a store, a handful of status calls, and a portable view.
- The `unpeel-ui` crate's `examples/tabs_canvas.rs` is a complete dual-mode program; the repository's `protocol/` directory holds the `unpeel.ui/1` schema and conformance fixtures.

## Ship your own agent skill

Your app should ship a `skill.md` next to its manifest — instructions like this file, but for *your* app: when agents should reach for it, how its declared tools compose, pitfalls. Agents retrieve it through the Unpeel MCP `apps` domain's `skill` action, so keep it tight and factual; it enters an agent's context only on demand.

## Checklist before calling it done

- [ ] Runs and is useful in a bare terminal with no `UNPEEL_*` env.
- [ ] Binary named `unpeel-<name>`, installable onto PATH.
- [ ] Status calls at real transitions; no polling loops, no fake busy.
- [ ] Standard keys (j/k, palette, help) and shared styles.
- [ ] No IDE chrome, no Node, no daemon, no writes outside its own data dir.
