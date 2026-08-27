The App contract is protocols and files, never a language. An App is anything launchable as a command that draws in a terminal; the Unpeel integrations are small, documented surfaces you can speak from any language. Rust with the `unpeel-ui` crate is the golden path — it is what first-party Apps use — not a requirement.

Building with an agent? The whole contract fits in one skill file: install it with

```sh
mkdir -p ~/.claude/skills/unpeel-apps && curl -fsSL https://unpeel.com/apps/skill.md -o ~/.claude/skills/unpeel-apps/SKILL.md
```

or just tell your agent to read [unpeel.com/apps/skill.md](https://unpeel.com/apps/skill.md).

## Standalone first

Build a complete CLI tool. It must install, run, and be useful in a bare terminal with no Unpeel present — that is the whole adoption story, and it mirrors how the agent CLIs themselves relate to Unpeel. Never gate your App's core function on Unpeel being around, and never require Unpeel-only setup before first run.

Inside Unpeel, your App is launched as a hosted session with environment it can detect (`UNPEEL_SESSION_ID` and friends). Outside, that environment is absent and every integration call should quietly do nothing.

## The golden path: `unpeel-ui`

Apps are primarily built with [`unpeel-ui`](/docs/unpeel-ui), a Rust crate on top of [Ratatui](https://ratatui.rs) that makes an App feel like part of the family:

- the shared style layer — colors, status styling, list/detail conventions
- fuzzy-scored palettes and the standard keybindings (j/k, palette, help)
- environment detection: one call tells you whether Unpeel is present
- a status reporter that posts activity (busy, idle, needs-you) and a short status line to the sidebar — every call a silent no-op when running standalone, so your code has zero `if unpeel` branches
- a portable view layer: one view definition renders in any terminal today and serializes over `unpeel.ui/1` so native renderers can draw the same interface later

`unpeel-todos` is the reference App built on it, and its source is the fastest way to see the shape: a plain Ratatui program with a store, plus a handful of status calls. The [unpeel-ui doc](/docs/unpeel-ui) covers the SDK in detail.

## Teach agents your app: the app skill

Ship a `skill.md` next to your manifest — prose instructions for agents: when to reach for your app, how its tools compose, conventions and pitfalls. Agents fetch it on demand through the built-in Unpeel MCP (`apps` domain, `skill` action), so it costs nothing in context until an agent actually decides to work with your app. Think of it as your app's man page for agents: the tool description carries one line, the skill carries the craft.

## Sidebar activity and status

Two small surfaces give your App a live sidebar row:

- **Activity** rides the same hook events agent integrations use — your App reports busy when working, idle when done, attention when it needs the user. Unpeel's activity engine, unread badges, and phone notifications then work unmodified.
- **Status text** is a single short line ("3 open · 1 done") written to a per-session marker and rendered under your App's sidebar row on desktop, in the terminal UI, and on the phone.

## Distribution

Ship a binary. When it is on the user's PATH, Unpeel's startup scan recognizes it (the `unpeel-*` naming convention) and seeds a preset entry — once. After that the entry belongs to the user: if they hide or remove it, it stays gone; if they uninstall the binary, the preset goes launch-dead like any command that left the PATH. No store, no registration, no manifest server.

## Rules that apply to every App

- **Never IDE chrome.** No diff views, file trees, or editor panes — first-party or third-party, surface or panel or widget.
- **No Node.** Unpeel never ships or requires a Node runtime. What you run in your own environment is your business, but no first-party App uses it.
- **No daemons.** An App instance is a hosted session owned by Unpeel's host process — manifest, heartbeat, cleanup — never a background service of its own.
