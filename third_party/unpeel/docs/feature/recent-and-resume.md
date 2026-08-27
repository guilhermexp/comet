# Recent And Resume

## Summary

Unpeel keeps both app-owned saved sessions and provider-owned recent session history.

This allows the app to reopen old work, discover provider session refs, and construct provider-aware resume commands.

## Main Files

- `apps/native/UnpeelNative/Sources/UnpeelNative/ResumeCommand.swift`
- `apps/native/UnpeelNative/Sources/UnpeelNative/Presets.swift`
- `crates/unpeel-core/src/transcripts.rs`

## Stored State

- `saved_sessions`
  - app-level session records that can still be shown or resumed
- `recent_sessions`
  - provider session records keyed by tool and provider session ref

## Resume Behavior

Unpeel prefers a known `tool_session_id`.

If it is missing, runtime discovery scans provider files and tries to infer the right session ref from:

- the command
- the project path
- timestamps
- preview text

## Current Resume Commands

- Claude: `claude --resume <id>`
- Codex: `codex resume <id>`
- Cline: `cline --id <id>`
- Gemini: `gemini --resume <id>`
- Pi: `pi --session <id>`
- Kimi: `kimi --session <id>` once SessionStart reports Kimi's provider-created id
- OpenCode: `opencode --session <id>`

Cline's native global TaskStart hook captures the persisted root session id
when the first run begins.
`~/.cline/data/sessions/<id>/<id>.messages.json` is the canonical transcript.
If an older Unpeel session has no captured Cline id, Restart opens
`cline history`; Cline exposes no non-interactive continue-last flag.

## Current Provider Discovery

Current Kimi Code stores the main-agent transcript at
`~/.kimi-code/sessions/<work-dir-key>/<session-id>/agents/main/wire.jsonl` and
indexes it in `~/.kimi-code/session_index.jsonl`; legacy Kimi uses
`~/.kimi/sessions/<md5(canonical-cwd)>/<session-id>/context.jsonl`. Current
Kimi creates its own session id, which Unpeel captures from SessionStart and
uses for exact restart and transcript lookup. Before an id has been captured,
restart falls back to `--continue`.
