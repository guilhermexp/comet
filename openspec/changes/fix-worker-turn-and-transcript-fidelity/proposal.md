# Proposal

## Why

Incident 2026-10-14, Worker `5723366d-72a4-4f80-b8e2-39e8935ee312` (Claude): the Orchestrator was told `completed` at 17:39 and 17:40 while the Worker still waited on background shells and agents. Its final 20 425-character report reached the Orchestrator cut to ~4 000 characters. The Orchestrator then asked the Worker to save the report to a temporary file. When the user interrupted that request with Escape, the Worker stayed `working`.

Root causes, all confirmed against the session files:

- Claude fires `Stop` when it ends a turn with `run_in_background` shells or async agents outstanding. It reopens a turn when they report back. Completion evidence treated every quiescent `Stop` as task completion: 8 of the 9 `Stop`s in that session had pending tasks.
- Claude fires no hook on a user interrupt. When the prompt returns to the composer, it paints no interrupt row either. The Host's settled-interrupt detector recognizes only Codex's `■ Conversation interrupted`.
- The shared transcript reader truncated every message to 4 000 characters before rendering. "Whole transcript" was clamped to the last 2 MiB of the JSONL. The hub ignored the saved range (it always used 50 and refused 0) and cut the Markdown to 96 KiB with no way to continue.
- The controller prompt did not forbid pushing Comet's own failures onto the Worker.

## What Changes

- Completion evidence also requires that no background task from the current runtime is still pending in the provider transcript. After 15 minutes of output silence, a pending task no longer withholds completion, so a background dev server cannot hold a task open forever.
- The Host stamps `interrupt-requested` on every bare Escape written to the PTY. While that stamp is newer than the last lifecycle hook, the turn reads as settled (report-only; it never confirms Stop). `wait_for_status(completed)` returns `interrupted: true` instead of running out its timeout.
- Messages are never truncated in the shared reader. "Whole transcript" reads up to 64 MiB and adds an explicit note beyond that.
- `read_transcript` honors the saved range, accepts `0`, and pages with `offset`, `next_offset` and `previous_offset`.
- The controller description and `help` state that `read_output` is the terminal screen only. Tool failures are Comet's to fix; messages to a Worker carry only guidance about its task.

## Not changed

- The brief is submitted once: the native transcript has one initial user entry. The second copy seen in the terminal is Claude repainting its scrollback after resize. `read_output` is now documented as screen-only; message deduplication is deliberately not added.
- Hook-journal retention (events compacted on acknowledgement) is intentional and unchanged.

## Capabilities

### New Capabilities

- `worker-turn-and-transcript-fidelity`

## Impact

`crates/workers-unpeel` (`background_tasks`, `parent_notifications`, `activity_bridge`, `controller_mcp`) and vendored `unpeel-core` (`transcripts`, `session_host`). There is no wire or CRDT change. MCP `read_transcript` gains the optional `offset` argument and new page fields. Running session Hosts pick up the Escape stamp only after they restart.
