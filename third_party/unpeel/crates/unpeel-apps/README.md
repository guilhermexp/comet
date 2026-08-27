# unpeel-apps

First-party **Unpeel Apps**: standalone-first terminal CLIs that light up
inside Unpeel with sidebar activity, a status line, and — as the portable
rendering path lands — native and phone rendering of the same view tree.

Every crate in this folder follows the App contract
(`docs/plans/unpeel-apps.md` is authoritative; user-facing docs at
`unpeel.com/docs/unpeel-apps`):

- **Standalone first.** The binary installs, runs, and is fully useful in a
  bare terminal with no Unpeel present. Unpeel enhances it; it is never a
  prerequisite.
- **Named `unpeel-<name>`.** Unpeel's startup PATH scan recognizes the prefix
  and seeds a launch preset — that is the whole distribution story.
- **Built on [`unpeel-ui`](../unpeel-ui/)** — the golden path SDK: shared
  style/keys/fuzzy helpers, host detection, the no-op-when-standalone status
  reporter, and the portable `unpeel.ui/1` view layer.
- **Agent-usable by declaration.** The manifest declares agent tools and an
  optional `skill.md` (how-to-use-me prose agents fetch on demand via the
  `unpeel` MCP `apps` domain) — an app never runs its own MCP server.
- **Never IDE chrome, no Node, no daemons.** Apps are hosted sessions owned
  by Unpeel's host process and stay within Unpeel's review-surface rules.

## Apps

| Crate | What it is |
| --- | --- |
| [`unpeel-todos`](unpeel-todos/) | The reference App: a complete standalone todo TUI with sidebar status ("3 open · 1 done") and a portable view. Read this first. |

## Working here

```sh
# Run an app standalone from the workspace:
cargo run -p unpeel-todos

# Test everything in this folder:
cargo test -p unpeel-todos
```

New first-party Apps live here as `unpeel-apps/unpeel-<name>` workspace
members (planned next: `unpeel-chat`, `docs/plans/chat-sessions.md`). Building
an App outside this repo? Any language that draws in a terminal qualifies —
start at `unpeel.com/docs/building-apps`, or hand your agent
`unpeel.com/apps/skill.md`.
