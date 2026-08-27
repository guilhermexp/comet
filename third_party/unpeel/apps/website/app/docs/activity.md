Running several agents only works if you can tell, at a glance, which ones need you. Unpeel tracks every session's state and surfaces it in three places: the sidebar, the app icon, and the menu bar.

## The three states

- **Busy** — the agent is working. The session shows a live spinner, tinted per provider.
- **Done** — the agent finished its turn and is waiting for you. If it finished while you were looking elsewhere, the session keeps an **unread** badge until you view it.
- **Needs you** — the agent is blocked on a permission prompt or a question. This is the state that matters most, and it's visually loudest.

## How Unpeel knows

For managed launches of hook-capable agents (Claude, Codex, Cline, Gemini, Amp, OpenCode, Cursor, Grok, Kimi, Kiro, Copilot — see [Supported agents](/docs/agents)), the CLI reports lifecycle events the moment they happen: turn started, turn finished, and — where the provider exposes it — permission requested. This is precise — a full-screen TUI repainting, or you scrolling, never fakes a busy state. Cline's native hook surface does not expose approval requests, so custom non-auto-approved Cline presets rely on the visible terminal prompt for that one state.

Agents without hooks fall back to output heuristics: growing terminal output means busy, quiet means idle. The same honest fallback applies when Unpeel [observes an agent inside a blank Terminal](/docs/agent-runtimes#observed-later-in-a-blank-terminal) without a managed hook binding.

Status survives app restarts too: hook events are recorded on disk, so if an agent finished while Unpeel was closed, it shows as done — not stuck busy — when you reopen.

## The menu bar

The menu-bar item is the whole fleet in one glyph: it **spins** while any agent is working and **rings** when one needs you. Click it for the full roster grouped by state, pick a session, and the window snaps open to that conversation — including reopening the app if you'd closed the window entirely.

Because sessions [outlive the window](/docs/sessions), the menu bar is how many people run Unpeel most of the day: window closed, agents working, one glance now and then.
