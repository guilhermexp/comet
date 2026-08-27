The core idea: **the window is not the terminal.** Every session runs in its own small host process that owns the real terminal, writes every byte to disk, and keeps running whether or not the app is open. The Unpeel window just attaches to it — the same client/server split people love tmux for, built in and invisible.

## Sessions survive the app

Quit Unpeel, crash it, update it, relaunch it — running sessions keep working the whole time. On startup, Unpeel finds the live session hosts on disk, replays each terminal's saved output so its scrollback is intact, and reconnects. Nothing about a running agent depends on the window staying open.

What each session persists:

- the full terminal output log (that's what gets replayed when you reattach)
- its metadata: title, project, created time, running state

## Auto-titling

Sessions title themselves from your first prompt, so the sidebar reads like a list of tasks instead of a list of shells. Rename a session any time — a manual title always wins over auto-titling. You can also **pin** sessions you keep coming back to.

## Copy a transcript

Right-click any session in the sidebar ▸ **Copy transcript** and pick a range — the last 20 or 50 entries, or the whole conversation — to put it on your clipboard as clean **Markdown**: user and assistant turns, ready to paste into a doc, an issue, or a message. On the phone, the same action (with the same range picks) lives in the session's edit sheet (long-press the terminal title bar).

You decide what a transcript includes in **Settings ▸ Transcripts**:

- **Session info** — start the transcript with a header carrying the session's title, ID, CLI, and model, so a pasted transcript says where it came from (and another agent can target the session by ID).
- **Content** — turn each part on or off: user messages, assistant messages, reasoning, tool calls & results, file changes & diffs, and plan updates. By default you get just the user and assistant conversation; flip on the rest when you want the full working record.
- **Range** — the whole conversation, or only the most recent 20 / 50 / 100 entries.

These are the same options an agent sees through the [Sessions MCP](/docs/sessions-mcp) `read_transcript` tool, so what you copy and what an agent reads stay in sync.

## Resume Agent = continue in the same terminal

When a managed agent exits or crashes back to its still-live shell, **Resume
Agent** relaunches the original command **with the provider's resume flag
injected**. The Session, Host, PTY, socket, scrollback, artifacts, and terminal
identity stay in place while the conversation continues. This preserves the
saved terminal output; it does not promise an entry in the shell's Up-arrow
command history. Resume Agent is not offered while the runtime is active and
never stops it. Before showing or running the action, Unpeel verifies that the
original interactive login shell has the foreground and no retained or
background agent job or different recognized runtime remains:

- **Claude Code, Gemini, and Grok** resume precisely, always: Unpeel assigns the conversation id itself at launch with `--session-id <uuid>`, so the resume recipe targets that exact conversation even if the provider crashes seconds after starting.
- **Kimi, Cline, Codex, Amp, OpenCode, Cursor Agent, Kiro, Muse Code, and Copilot** resume precisely once their hooks or plugins report a conversation id (`kimi --session <id>`, `cline --id <id>`, `codex resume <id>`, `amp threads continue <id>`, `opencode --session <id>`, `cursor-agent --resume <chatId>`, `kiro-cli --v3 --resume-id <id>`, `muse resume <id>`, `copilot --resume <id>`). Current Kimi creates its own `session_<uuid>` id and reports it through SessionStart; Cline reports its persisted root id from TaskStart; Muse Code reports its id from SessionStart.
- **Pi** keeps no visible conversation id, so Unpeel gives each pi session its own private session storage instead — Resume Agent's `--continue` then always lands on the right conversation, even with several pi sessions in one folder.
- **Any session without a captured id yet** falls back to the provider's "continue most recent" flag. For sessions in their own [worktree](/docs/worktrees) this is exact; in a shared project folder it continues whichever conversation ran there most recently. Cline has no continue-last flag, so an older Cline session without an id opens its history picker for an explicit choice.

Because the Session itself is unchanged, its title, pin, worktree, sidebar group, and remote identity stay attached to it.

A blank Terminal has a different stable launch: the shell itself. If Unpeel recognizes Claude or Codex after you type it there, that live observation does not rewrite the Session into an agent preset or enable Resume Agent. If that terminal later has to be restored, it opens the original blank shell. See [Agent runtimes](/docs/agent-runtimes) for the capability boundary and manual MCP option.

## Append system context

Right-click a session ▸ **Append system context…** to give the agent standing instructions — tone, constraints, project facts — on top of whatever it already knows. Agents read their system prompt once at launch, so the context is saved first and applied by the next Resume Agent after the runtime returns to its shell. Unpeel never asks you to interrupt an active agent to apply it.

Supported per CLI: **Claude Code** (`--append-system-prompt`), **Grok** (`--rules`), **Codex** (a `developer_instructions` override), and **Pi** (`--append-system-prompt`). These all *add* to the agent's instructions — Unpeel never replaces a CLI's built-in system prompt.

## Resume and reload recommendations

Long-lived sessions can outlive an app update or accumulate context that applies only at agent launch. When that happens, Unpeel shows a quiet bar above the terminal — it never kills a running agent out from under you. Pending launch context waits until **Resume Agent** becomes available after the agent returns to its shell. **Reload Terminal** is the separate maintenance action for a Host update or recovery that really must replace the terminal. Dismiss the bar and it returns only for a new reason.

## Limits worth knowing

If only the provider process crashes and the shell survives, use **Resume
Agent** in the same terminal. If the Host process itself dies—a reboot or a
`kill`—the terminal is gone. Its saved output remains readable until you choose
ordinary **Resume**, but only after Unpeel proves the old Host child is gone or
its PID was recycled. Healthy or uncertain process ownership fails closed and
is not replaced. Resume then starts a replacement Host with the provider's
conversation recipe; the old terminal scrollback is not carried into the
replacement. The provider conversation and transcript continue, not the dead
PTY. An archived resumable Session exposes **Restore & Resume** with the same
replacement behavior; an unknown/non-resumable command can only be restored.
