# Agentation Integration

Connect to a running [Agentation](https://www.agentation.com/api) instance and pipe annotation data directly into the active terminal.

## Overview

User configures an Agentation URL (e.g. `http://localhost:3000`). Unpeel connects via SSE, listens for events, and writes annotation data into the active terminal's xterm input — useful for feeding instructions to AI agents running in the terminal.

## Data flow

```
Agentation server → SSE /events → agentation store → sessionController.enqueueInput() → terminal
```

## Agentation API surface (relevant endpoints)

- `GET /events` — Global SSE event stream (optional `?domain=` filter)
- `GET /sessions/:id/events` — Session-specific SSE stream
- `GET /pending` — All pending annotations
- `GET /sessions/:id/pending` — Pending annotations for a session
- `PATCH /annotations/:id` — Update annotation (mark as handled)

Event types: `annotation.created`, `annotation.updated`, `action.requested`, `thread.message`

## Implementation plan

### Rust backend (3 files)

1. **`src-tauri/src/state.rs`** — Add `agentation_url: String` to `AppState` (`#[serde(default)]`, empty = disabled)
2. **`src-tauri/src/appearance.rs`** — Add `get_agentation_url` / `set_agentation_url` Tauri commands
3. **`src-tauri/src/lib.rs`** — Register the two new commands

### Frontend (5 files)

4. **`src/lib/stores/agentation.ts`** *(new)* — Svelte store:
   - Holds `agentationUrl` (loaded from backend on startup)
   - Manages `EventSource` (SSE) connection to `{url}/events`
   - Exposes connection status: `disconnected | connecting | connected`
   - Exposes pending annotations count
   - On `annotation.created` / `action.requested` events, writes the annotation comment into the active terminal via `sessionController.enqueueInput()`

5. **`src/lib/AgentationButton.svelte`** *(new)* — Floating button, bottom-right (left of scroll-to-bottom button, `right: 64px`):
   - Shows connection status (green dot = connected, gray = off)
   - Badge with pending annotation count
   - Click to list pending annotations, each with a "paste" action

6. **`src/lib/TerminalView.svelte`** — Add `AgentationButton` next to scroll-to-bottom `GlassButton`

7. **`src/lib/settings/GeneralPanel.svelte`** — Add "Agentation URL" text input below Code Editor setting

8. **`src/lib/settings/tabs.ts`** — No change needed (fits in General panel)

## What gets written to the terminal

The annotation `comment` field, followed by `\r` (enter). This works naturally with AI agents (Claude, Codex) — the annotation becomes a prompt/instruction.

## Auto-discovery from session files

Instead of (or in addition to) manual URL config, Unpeel could auto-detect a running Agentation instance by scanning tool session files that it already reads.

### How it works

Unpeel already parses session JSONL files for Claude, Codex, Gemini, Pi, and Kimi (see `daemon.rs`) to discover sessions, extract titles, and enable resume. The same files often contain output showing background services being started:

```
Agentation server running at http://localhost:3000
```

Or user prompts like:

```
start agentation on port 3000
```

### Detection approach

1. When a session becomes active, scan its corresponding tool session file (already located via `tool_session_id` + `session_files_for_tool()`)
2. Grep for URL patterns matching `https?://localhost:\d+` or known Agentation patterns
3. Optionally hit the candidate URL's `/health` endpoint to confirm it's an Agentation instance
4. If confirmed, auto-connect SSE and show the Agentation button

### Relevant existing code

- `session_files_for_tool(tool, cwd)` in `daemon.rs` — already finds the right JSONL files per tool
- `tool_session_id` on `SessionInfo` — links a Unpeel session to its tool session file
- Claude files: `~/.claude/projects/{encoded_cwd}/{session_id}.jsonl`
- Codex files: `~/.codex/sessions/**/*.jsonl`

### Benefits

- Zero config — user launches Agentation from within Claude/Codex and Unpeel picks it up automatically
- Per-session — different sessions could have different Agentation instances
- Safe — health check confirms the URL is actually Agentation before connecting

## Open questions

- Should we auto-write on event, or require manual paste via the button?
- Should annotations be marked as handled (PATCH) after pasting?
- Auth support — API currently has none, but may need an API key header later
- Filter by domain? The SSE endpoint supports `?domain=` filtering
- Auto-discovery: scan only active session file, or all recent files?
- Auto-discovery: how often to re-scan (once on session switch, or periodic?)
