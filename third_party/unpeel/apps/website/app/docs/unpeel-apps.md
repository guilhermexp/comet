An Unpeel App is a terminal program — a task list, a notes tool, a dashboard — that Unpeel launches, persists, streams, and remotes exactly like an agent session. Agents made the terminal Unpeel's universal surface; Apps apply the same idea to tools, for any domain. Never code-editor chrome: no App gets diff views, file trees, or editor panes.

Every App works standalone first. Install one, run it in any bare terminal with no Unpeel present, and it is a complete tool. Running it inside Unpeel is what adds the superpowers — nothing about its core function ever requires Unpeel.

The first App is `unpeel-todos`: a todo list that runs anywhere and reports "3 open · 1 done" to your sidebar when Unpeel is around.

## What Unpeel adds

- **It just appears.** Install an App and it shows up in your preset list, ready to launch — no wizard, no import step. From then on the entry is yours: reorder it, star it into a sidebar chip, or hide it, and it stays that way.
- **Sidebar presence.** Apps feel alive the way agent sessions do: a spinner while working, attention when they need you, and a short status line under the row.
- **Everything sessions get.** Apps survive app restarts, archive and restore, and stream to your iPhone and iPad over the same remote connection your agents use.

## Three places an App can appear

One App, three placements — the same tool renders differently depending on where you put it:

- **Surface** — the full session area, like any agent session. This is how Apps run today.
- **Panel** — a column beside a session: a task list or dashboard next to the agent working on it. Think of an artifact pane, kept to Unpeel's review surfaces — rendered docs, task lists, dashboards — never diffs or file trees.
- **Widget** — a compact tile in an optional always-on rail: a persistent column of small App views with resizable panes. Widgets come in two tiers: a glanceable status tile that costs no running process, and a live pane you expand when you want the full mini-app. Widgets can be pinned globally or scoped to a project, so the rail follows the project you're working in.

Panels and the widget rail are roadmap work; surfaces, sidebar status, and preset injection are where Apps start.

## What Apps never are

Apps run on your machines under the same rules as everything else in Unpeel. There is no app store, no cloud runtime, no background daemon, and no Node runtime shipped or required. An App is a binary on your PATH; distribution is installing it.

## Agents can use Apps too

Agents in Unpeel sessions reach installed Apps through the built-in Unpeel MCP: discover them, open content in them, call the tools an App declares — and read the App's *skill*, its shipped instructions for agents, loaded only when needed. An App integrates with agents by declaring, never by running its own MCP server.

## Build one

Any language that can draw in a terminal can be an App — see [Building an app](/docs/building-apps). First-party Apps are built with [the `unpeel-ui` SDK](/docs/unpeel-ui), the Rust golden path: shared style and keybindings, sidebar status with zero Unpeel-specific branches, and one view definition that renders in any terminal today and is ready for native rendering next.
