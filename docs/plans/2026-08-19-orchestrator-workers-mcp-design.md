# Orchestrator Workers MCP Design

## Goal

Give the primary Comet Orchestrator a provider-neutral MCP surface for launching,
observing, messaging, waiting for, stopping, and archiving CLI Workers.

## Product contract

- The MCP is injected only into the primary ACP agent session.
- Worker CLIs do not receive the controller MCP and cannot call `launch_worker`.
- The existing Unpeel `unpeel` MCP remains unchanged for its own session/browser
  domains.
- Comet owns the new server and implementation under the MIT repository; no GPL
  MCP implementation is copied or modified.
- Launches use registered projects and enabled presets by default. Raw commands
  remain available because the Workers UI already supports them.
- Destructive actions are bounded to `stop` and `archive`; removal is not exposed.
- The server is local stdio only and is a cooperative same-UID control, not an OS
  sandbox boundary.

## Considered approaches

### 1. Extend Unpeel's MCP host

This would reuse upstream actions, but the host requires a worker caller manifest
and deliberately forbids session creation. Adding a controller identity would
modify the GPL submodule and couple Comet policy to upstream internals. Rejected.

### 2. Add provider-specific native tools

Each ACP adapter could expose direct Comet callbacks. This duplicates behavior per
provider and makes capability parity depend on each adapter. Rejected.

### 3. Add a Comet-owned stdio MCP server

Chosen. `zeron-workers-unpeel` exposes a small `comet-workers` server backed by
`LocalWorkersClient`. The ACP harness injects the same stdio descriptor into every
primary-agent run. Discovery probes remain free of the server.

## MCP surface

The server advertises one action-enum tool named `workers` to keep context cost
small. It supports:

- `help`
- `list_projects`
- `list_presets`
- `launch_worker`
- `list_workers`
- `inspect_worker`
- `read_output`
- `read_transcript`
- `send_text`
- `send_keys`
- `wait_for_status`
- `stop_worker`
- `archive_worker`

`launch_worker` accepts a project ID plus either a preset ID or raw command, with
optional initial text and optional worktree metadata. It returns the exact session
ID. It never selects a UI row or changes the current Workers view.

Reads return compact JSON text with stable IDs. Output reads cap returned bytes,
strip terminal escape sequences, and never expose process argv beyond existing
session command metadata. Transcript reads reuse the app-wide transcript renderer.

`send_keys` accepts a bounded list of named keys (`enter`, `escape`, arrows, `tab`,
`backspace`, `ctrl-c`, and printable text). `wait_for_status` polls the authoritative
Workers bootstrap with a maximum 120-second timeout and returns the final session
snapshot on success or timeout.

## ACP injection

The harness builds this ACP `session/new` descriptor for actual Orchestrator runs:

```json
{
  "type": "stdio",
  "name": "comet-workers",
  "command": "/absolute/path/to/zeron",
  "args": ["__workers_mcp__"],
  "env": [
    {"name": "COMET_WORKERS_CONTROLLER", "value": "1"}
  ]
}
```

The descriptor is used for both `session/new` and `session/load`, so resumed main
agent sessions retain the capability. It is omitted from command/model discovery
sessions and when `ZERON_DISABLE_WORKERS_MCP=1` is set.

The executable resolves from `ZERON_WORKERS_MCP_BIN` when explicitly set, otherwise
the current Comet executable. The MCP server refuses startup unless the controller
environment marker is present. This prevents accidental invocation but is not
described as a security boundary against same-user code.

## Data flow

```text
Orchestrator ACP session
  -> stdio MCP `comet-workers`
  -> action validation and bounds
  -> LocalWorkersClient
  -> existing controller/session-host APIs
  -> Workers manifests, PTYs, transcripts, and lifecycle actions
```

The server does not depend on GPUI. It keeps working while the window is closed and
does not add UI polling or permanent visual elements.

## Error handling

- Unknown project, preset, worker, action, or key returns an MCP tool error.
- Launch requires exactly one of preset ID or command.
- A target disappearing during wait returns an explicit error.
- Output/transcript payloads are capped before returning to the model.
- Timeouts include the last observed activity and do not alter the worker.
- Stop/archive failures preserve the original session state.
- Malformed JSON-RPC requests return standard parse/invalid-request errors without
  terminating the server.

## Testing

- Pure action parser and key encoder tests.
- MCP JSON-RPC initialize, tools/list, unknown action, and startup-gate tests.
- Local fixture integration: create project, launch terminal worker, list/inspect,
  send text, read output, stop, and archive.
- ACP fixture test proving primary runs receive `comet-workers`, while discovery
  sessions receive an empty MCP list.
- Full Workers, harness, engine, check, and build gates.
- Native dev validation: ask the Orchestrator to list projects, launch a safe shell
  worker, wait for output, stop/archive it, and inspect the terminal history.
