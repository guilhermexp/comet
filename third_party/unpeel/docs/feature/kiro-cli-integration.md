# Kiro CLI integration audit

Audited against Kiro CLI `2.13.0` on 2026-07-17. Unpeel treats the v3 engine as
first-class and ships the built-in command:

```sh
kiro-cli --v3
```

Official references:

- [Kiro CLI v3](https://kiro.dev/docs/cli/v3/)
- [V3 hooks](https://kiro.dev/docs/cli/v3/hooks/)
- [V3 agent configuration](https://kiro.dev/docs/cli/v3/agent-config/)
- [CLI session management](https://kiro.dev/docs/cli/chat/session-management/)
- [MCP configuration](https://kiro.dev/docs/cli/mcp/configuration/)
- [CLI commands](https://kiro.dev/docs/cli/reference/cli-commands/)
- [ACP](https://kiro.dev/docs/cli/acp/)

## Capability matrix

| Unpeel capability | Kiro support | Integration |
| --- | --- | --- |
| Durable terminal | Full | Hosted PTY is provider-independent. |
| Lifecycle status | Full in v3 | Global `SessionStart`, `UserPromptSubmit`, `PreToolUse`, `PostToolUse`, and `Stop` hooks. |
| Global hooks | Full in v3 | Managed `~/.kiro/hooks/unpeel.json`; other global/workspace hook files are preserved. |
| Legacy hooks | Compatibility | Managed `unpeel-runtime` v2 agent embeds lower-camel-case hooks. |
| Exact restart | Full after session start | Captured `sess_…` id is relaunched with `--resume-id`. |
| Resume fallback | Full | `--resume` continues the latest conversation for the cwd. |
| Transcript | Full | Both v3 `messages.jsonl` and v2 TUI JSONL are normalized, including tools. |
| Sessions MCP | Full | One grant-aware combined Kiro MCP server. |
| Browser MCP | Full | Advertised by the same server only when Browser Access is enabled. |
| Default Kiro agent | Preserved | V3 does not select an Unpeel custom agent, because doing so replaces Kiro's built-in system prompt. |
| Fork | Unsupported | Kiro has no external CLI fork primitive that creates an independent conversation. |
| Append system context | Unsupported | Agent prompts replace the base prompt; there is no append-only CLI flag. |
| Approval attention hook | Unsupported in v3 | V3 exposes no permission-request hook. Pre-tool hooks are not treated as approvals because they fire for every tool. |
| Preassigned new session id | Unsupported | Kiro exposes `--resume-id`, but no flag to assign the id of a new session. |

## Hook contract

V3 global hooks are JSON files under `~/.kiro/hooks/`. Unpeel owns only
`unpeel.json` and installs five command hooks:

| Kiro trigger | Unpeel event | Purpose |
| --- | --- | --- |
| `SessionStart` | `HookSeen` | Capture provider session id without marking the turn busy. |
| `UserPromptSubmit` | `UserPromptSubmit` | Start a turn. |
| `PreToolUse` | `Start` | Keep the session working while a tool runs. |
| `PostToolUse` | `Start` | Keep the session working between tool calls. |
| `Stop` | `Stop` | Mark the turn complete and power notifications. |

Kiro command hooks receive JSON on stdin. The installed script forwards
`session_id`, `cwd`, `tool_name`, and the resolved transcript path. Because v3
hooks are global, the script intentionally does nothing when
`UNPEEL_SESSION_ID` is absent; ordinary Kiro sessions outside Unpeel are not
posted into the app.

The v2 engine has no global hook directory. For commands that do not select v3
or another custom agent, Unpeel adds `--agent unpeel-runtime`. That managed
agent keeps `prompt: null`, `tools: ["*"]`, `includeMcpJson: true`, and embeds
the equivalent `agentSpawn`, `userPromptSubmit`, `preToolUse`, `postToolUse`,
and `stop` hooks.

## Session files and transcripts

V3 derives a workspace directory from the first 16 hex characters of the
SHA-256 digest of the canonical cwd:

```text
~/.kiro/sessions/<sha256(canonical-cwd)[0..16]>/<sess_id>/
  session.json
  messages.jsonl
  publish.cursor
```

The v3 JSONL envelope uses `payload.type` values such as `user`, `assistant`,
`tool_call`, and `tool_result`. Unpeel ignores internal turn, hook, usage, and
system-prompt records.

The v2 TUI store is:

```text
~/.kiro/sessions/cli/<session-id>.json
~/.kiro/sessions/cli/<session-id>.jsonl
~/.kiro/sessions/cli/<session-id>.history
```

Its JSONL uses `Prompt`, `AssistantMessage`, and `ToolResults` records. Unpeel
parses both formats so upgrading a preset to v3 does not make older sessions
unreadable.

Classic non-TUI conversations use Kiro's SQLite database at
`~/Library/Application Support/kiro-cli/data.sqlite3`. They are not the
first-class Unpeel launch path; the built-in preset and compatibility TUI path
both use JSONL-backed sessions.

V3 and v2 session formats are separate and not cross-resumable. Unpeel resumes
the captured id with the same command/engine that created it.

## MCP

Kiro reads global MCP configuration from `~/.kiro/settings/mcp.json` and
workspace configuration from `.kiro/settings/mcp.json`; it has no additive
per-launch MCP config flag.

Unpeel therefore merges one `unpeel` stdio server into the global file and
preserves every other server. It runs the provider-neutral
`unpeel-host __mcp_gate__ unified` gate, which combines Sessions MCP and Browser
MCP but filters its advertised tools from per-launch environment values
explicitly expanded by Kiro's MCP `env` configuration:

- `UNPEEL_KIRO_SESSIONS_MCP_ENABLED`
- `UNPEEL_KIRO_BROWSER_MCP_ENABLED`

This keeps grants exact per session even though the server registration itself
is global. Outside Unpeel, or with both grants disabled, the server initializes
normally and advertises zero tools.

## Installed CLI verification

The following were exercised against an authenticated local installation:

- installation and `kiro-cli 2.13.0`
- v3 launch with `--v3`
- global v3 hooks from `~/.kiro/hooks/`
- `SessionStart`, prompt, pre/post-tool, and stop payloads
- a real shell tool call
- exact `--resume-id sess_…` continuation
- v3 `session.json` and `messages.jsonl`
- v2 TUI hooks and JSONL storage
- classic non-interactive session listing and SQLite storage
- generated Unpeel v3/v2 hook assets in an isolated Kiro home
- combined MCP with zero grants and with both grants enabled
- Kiro's real `/mcp` panel with 29 tools through the generated environment map
- normalized transcript snapshots from real v3 and v2 session files
