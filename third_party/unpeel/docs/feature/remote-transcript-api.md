# Remote Transcript API

Last updated: 2026-06-23

## Purpose

The remote transcript API is a shared semantic read layer over provider-owned
conversation files. It lets the macOS app, iOS app, MCP tools, and future chat
surfaces read agent conversations without replaying or parsing the terminal
screen.

This is not replacing the iOS terminal view yet. The current mobile product
direction is still **terminal-first**:

- iOS should continue to show and control the live terminal as the primary
  session detail surface.
- The transcript API is available for lightweight previews, future native chat
  wrappers, debugging, and MCP `read_transcript`.
- A session must always have terminal fallback. Shells, setup screens, login
  flows, unsupported providers, and broken provider transcript resolution are
  terminal-only.

## Implementation Map

Core Rust module:

- `crates/unpeel-core/src/transcripts/`

Host CLI:

- `unpeel-host __transcript__ snapshot <session-id> [options]`
- `unpeel-host __transcript__ stream <session-id> [options]`
- `unpeel-host __transcript__ history <session-id> [options]`

MCP integration:

- `crates/unpeel-core/src/mcp_host.rs` uses the shared transcript module for
  `read_transcript` and transcript summaries in `inspect_session`.

iOS/shared protocol:

- `apps/shared/UnpeelShared/Sources/UnpeelShared/RemoteControlProtocol.swift`
  defines `RemoteTranscriptSnapshot`, `RemoteTranscriptStreamChunk`, and
  `RemoteTranscriptHistoryPage`.
- `apps/ios/UnpeelIOS/Sources/UnpeelIOS/RemoteMacClient.swift` exposes
  `transcriptSnapshot(...)`, `transcriptStreamChunk(...)`, and
  `transcriptHistoryPage(...)`.
- `apps/ios/UnpeelIOS/Tools/dev_bridge.py` exposes a dev-only HTTP endpoint:
  `GET /transcript?session_id=...&mode=snapshot|stream|history`.

(The old `provider-transcripts-chat-ui.md` deep-dive plan was deleted
2026-07-10 — this document is the current reference for transcript work.)

## Provider Sources

The transcript API reads durable provider storage when available. It does not
infer chat messages from terminal output.

| Provider | Status | Source |
| --- | --- | --- |
| Claude | Supported | `~/.claude/projects/<encoded-cwd>/*.jsonl` |
| Codex | Supported | `~/.codex/sessions/YYYY/MM/DD/*.jsonl` |
| Cursor Agent | Supported | `~/.cursor/projects/<encoded-cwd>/agent-transcripts/**/*.jsonl` |
| Gemini | Supported | `~/.gemini/tmp/<project>/chats/session-*.json` or `.jsonl` |
| Grok | Supported | `~/.grok/sessions/<encoded-cwd>/<session-id>/chat_history.jsonl` |
| Kimi | Supported | Current `~/.kimi-code/sessions/<work-dir-key>/<session-id>/agents/main/wire.jsonl`; legacy `~/.kimi/sessions/<md5(canonical-cwd)>/<session-id>/context.jsonl` |
| OpenCode | Detected, planned | SQLite under `~/.local/share/opencode/` needs an adapter |
| Amp | Terminal fallback | Durable transcript storage not verified |
| Pi | Terminal fallback | No hook/session transcript contract today |
| Shell | Terminal fallback | No semantic transcript source |

Resolver priority:

1. Trusted `provider_transcript_path` from the Unpeel session manifest.
2. `provider_session_id` from hooks.
3. Resume/session id parsed from the launch command.
4. Provider-specific cwd/time match.

Only provider paths under known trust roots are accepted from manifests.

## Snapshot Mode

Snapshot mode is for first paint and compact reads.

Example:

```bash
unpeel-host __transcript__ snapshot <session-id> --entries 50
```

Options:

- `--entries N`: maximum normalized entries to return.
- `--include-tools`: include tool calls, tool results, and reasoning blocks
  where the provider stores them.
- `--max-bytes N`: cap the tail read for JSONL sources. For JSON document
  sources, the document is read from the head.

The response is normalized JSON:

```json
{
  "session_id": "2f82...",
  "provider": "codex",
  "source": "manifest",
  "provider_session_id": "019eef...",
  "path": "/Users/me/.codex/sessions/...",
  "entries": [
    {
      "role": "User",
      "text": "Fix this bug",
      "blocks": [{ "kind": "text", "text": "Fix this bug" }]
    },
    {
      "role": "Tool",
      "text": "Patched src/App.swift",
      "blocks": [
        {
          "kind": "diff",
          "toolName": "apply_patch",
          "status": "success",
          "text": "--- a/src/App.swift\n+++ b/src/App.swift\n@@\n-old\n+new",
          "metadata": { "path": "src/App.swift", "additions": "1", "deletions": "1" }
        }
      ]
    },
    {
      "role": "Assistant",
      "text": "Patched it.",
      "blocks": [{ "kind": "text", "text": "Patched it." }]
    }
  ],
  "next_offset": 78417710,
  "updated_at": 1782210872661
}
```

The iOS dev bridge maps this into `RemoteTranscriptSnapshot` with camelCase
keys and stable `RemoteTranscriptEntry`/`RemoteTranscriptBlock` structures.

Entries keep a compact `role`/`text` fallback for MCP summaries and older
clients. New UI surfaces should prefer `blocks` when present. Supported block
kinds are `text`, `reasoning`, `toolCall`, `toolResult`, `permission`, `info`,
`fileChange`, `diff`, `planUpdate`, `usage`, and `attachment`.

## Stream Mode

Stream mode is the lightweight incremental path. It follows the same pattern as
Touchgrass: remember a byte offset and partial trailing line, then read only
new appended bytes.

Client state per session:

```text
provider path
byte offset
partial unfinished JSONL line
```

Request:

```bash
unpeel-host __transcript__ stream <session-id> \
  --offset 78417710 \
  --partial '{"type":"event_msg"' \
  --max-bytes 262144
```

Algorithm:

1. Resolve the provider transcript path.
2. Open the file and stat its current length.
3. If the file shrank below the requested offset, treat it as rotated or
   truncated and restart from zero.
4. If the unread range is bigger than `max_bytes`, skip forward to a bounded
   tail window and set `truncated = true`.
5. Read only the selected byte range.
6. Prefix the previous partial line.
7. Split on newline.
8. Keep the final unfinished line as the new `partial`.
9. Parse only complete JSONL lines into normalized entries.

Response:

```json
{
  "session_id": "2f82...",
  "provider": "codex",
  "source": "manifest",
  "offset": 78409518,
  "next_offset": 78417710,
  "partial": "",
  "truncated": true,
  "entries": [
    { "role": "Assistant", "text": "Working on it." }
  ],
  "updated_at": 1782210872661
}
```

The iOS bridge maps this into `RemoteTranscriptStreamChunk`.

## History Mode

History mode is the reverse paging path for scroll-up loading. It reads a
bounded JSONL window before the caller's current top offset and returns the
oldest retained entry offset plus the end offset for that page.

Request:

```bash
unpeel-host __transcript__ history <session-id> \
  --before-offset 78409518 \
  --entries 80 \
  --max-bytes 524288
```

Response:

```json
{
  "session_id": "2f82...",
  "provider": "codex",
  "source": "manifest",
  "offset": 78377120,
  "next_offset": 78409518,
  "truncated": true,
  "entries": [
    { "role": "User", "text": "Earlier prompt" }
  ],
  "updated_at": 1782210872661
}
```

The iOS bridge maps this into `RemoteTranscriptHistoryPage`.

## Performance Model

The cheap path is offset-based:

- No terminal replay.
- No ANSI parsing.
- No full-file scan after initial resolution.
- No provider search when the manifest already has a trusted transcript path.
- Parse only complete newly appended JSONL records.

Expected behavior:

- First resolve can cost more if the manifest lacks `provider_transcript_path`
  and the adapter must search provider folders.
- After resolution, polling should usually read only a few KB and parse in a
  few milliseconds.
- For production, cache resolved `(provider, path, provider_session_id)` per
  Unpeel session in the Mac app or host process.
- Use `64 KB` to `256 KB` read caps for incremental polling. Larger caps are
  fine for manual catch-up but should not be the default phone polling path.
- Polling every `250 ms` to `1000 ms` is reasonable for UI. A future watcher or
  SSE/WebSocket stream can wrap the same offset reader.

The current `dev_bridge.py` implementation shells out to `unpeel-host` per
request. That remains acceptable for simulator/dev snapshot, stream, and
history work. The production `GET /mobile/transcript-markdown` endpoint now
calls the Rust transcript module in-process through `controller_api` and the
panic-contained native C bridge; it no longer spawns a helper for that request.
Structured production streaming is still future work.

## iOS Usage Guidance

The iOS app should remain terminal-first for the first product pass.

Use terminal rendering for:

- The main session detail screen.
- Direct typing and key input.
- Raw shells and unsupported providers.
- Any situation where semantic transcript resolution fails.

Use transcript API for:

- Optional session preview text.
- Future native chat mode experiments behind a feature flag.
- Search/indexing over agent conversation text.
- Debug UI that explains which provider transcript was resolved.

Do not make transcript mode the only control surface. Provider transcript files
are read-only from Unpeel's perspective; control still goes through the hosted
PTY `session.sock` write/resize path.

## HTTP Dev Bridge

The iOS simulator bridge supports:

```text
GET /transcript?session_id=<id>&mode=snapshot&entries=50&include_tools=false
GET /transcript?session_id=<id>&mode=stream&offset=0&partial=&max_bytes=262144
GET /transcript?session_id=<id>&mode=history&before_offset=4096&entries=80
```

The bridge returns successful unresolved responses instead of HTTP failures for
unsupported providers:

```json
{
  "sessionID": "abc",
  "providerID": "amp",
  "resolved": false,
  "entries": [],
  "fallbackReason": "No amp transcript found..."
}
```

Clients should render terminal fallback when `resolved == false`.

## Tests

Rust:

```bash
cd crates
cargo test
```

Shared Swift protocol:

```bash
swift test --package-path apps/shared/UnpeelShared
```

iOS package:

```bash
apps/ios/test-ios.sh
```

Manual smoke:

```bash
cargo build -p unpeel-host
crates/target/debug/unpeel-host __transcript__ snapshot <session-id> --entries 3
crates/target/debug/unpeel-host __transcript__ stream <session-id> --offset 0 --max-bytes 8192
```

Dev bridge smoke:

```bash
python3 apps/ios/UnpeelIOS/Tools/dev_bridge.py --port 17662
curl -s 'http://127.0.0.1:17662/transcript?session_id=<id>&mode=snapshot&entries=2'
```

## Future Work

- Expose snapshot/stream/history through the production Host router; today only
  Markdown is on the production in-process path while `dev_bridge.py` retains
  its process-per-request development endpoints.
- Cache resolved transcript paths and invalidate on manifest/provider id
  changes.
- Add file watcher or SSE/WebSocket transport over the same offset reader.
- Add OpenCode SQLite adapter.
- Add richer structured blocks for tool calls and reasoning in the shared
  protocol. The current bridge maps normalized text entries into simple blocks.
- Add user-visible debug state: provider, source, resolved path, and fallback
  reason.
