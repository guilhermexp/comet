# Native Workers MCP Regression Design

## Problem

Claude Code and Codex previously ran through ACP, which injected the Comet Workers controller into each session's `mcpServers`. Their newer native drivers preserve `enable_workers_mcp` in `RunRequest` but never consume it, so native sessions cannot see `mcp__comet-workers__workers`.

## Decision

Keep both native drivers and mount the existing Comet-owned stdio controller per process. Do not write to `~/.claude`, `~/.codex`, project MCP files, or any other persistent user configuration.

- Claude Code receives one process-scoped `--mcp-config` JSON value containing `comet-workers`.
- Codex app-server receives process-scoped `-c mcp_servers.comet-workers.*` overrides.
- Both reuse the current executable, `__workers_mcp__`, `COMET_WORKERS_CONTROLLER=1`, and the optional `COMET_WORKERS_PARENT_CHAT_ID`.
- `ZERON_DISABLE_WORKERS_MCP=1` remains an authoritative opt-out.
- ACP and OMP remain unchanged.

## Integration state

The app is `NO_SDK`: Claude uses the installed CLI's native stream-json protocol directly rather than `@anthropic-ai/claude-agent-sdk`. This change intentionally preserves that architecture and uses Claude Code's supported `--mcp-config` surface instead of introducing an SDK migration outside the regression scope.

## Error behavior

When Workers is enabled, failure to resolve an absolute controller executable is a startup error rather than silently running without the promised tool. When disabled, no Workers arguments or configuration are emitted.

## Validation

- Unit contracts for enabled, disabled, parent-chat, and absolute-executable behavior.
- Existing Claude, Codex, ACP, OMP, engine-routing, and UI tests.
- Process-scoped configuration probes against installed Claude Code and Codex CLIs.
- A real controller MCP handshake and `tools/list` proving the `workers` tool is advertised.

