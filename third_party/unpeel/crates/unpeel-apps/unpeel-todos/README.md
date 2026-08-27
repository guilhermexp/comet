# unpeel-todos

The example app for [`unpeel-ui`](../../unpeel-ui): a complete standalone todo
TUI that runs in any bare terminal, showing what an Unpeel App looks like end
to end (plan: `docs/plans/unpeel-plugins.md`, Horizon A).

It demonstrates the standalone-first contract:

- **Works anywhere** — `cargo run -p unpeel-todos` in any terminal; no Unpeel
  required.
- **Lights up inside Unpeel** — when hosted in an Unpeel session it reports
  sidebar activity and a status line ("3 open · 1 done") through
  `unpeel_ui::status::StatusReporter`; outside Unpeel those calls no-op.
- **Uses the shared toolkit** — `unpeel_ui`'s re-exported ratatui plus its
  `keys`, `style`, and `fuzzy` helpers, so the app matches Unpeel's look and
  keybinding conventions without depending on anything else.

## Keys

- `a` / `n` — add a todo, `e` — edit, `Space` / `x` — toggle done
- `d` — delete, `J` / `K` — reorder, `/` — fuzzy filter, `q` — quit
- Mouse: click to select, double-click to toggle, drag to reorder

## Storage

A flat JSON file at `~/.unpeel/todos.json` (or `$UNPEEL_HOME/todos.json`),
written atomically via a temp-file rename.
