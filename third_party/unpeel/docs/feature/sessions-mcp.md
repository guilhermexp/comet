# Unpeel Sessions MCP

> **Model update (2026-08-10) — the original design sections below this notice
> are historical.** The current cooperative access policy is **open reads, same-group writes,
> approval for cross-group writes**:
>
> - **Reads are open across ALL sessions** (any project). The role/reach
>   ("Member/Orchestrator") grants described below are gone from the gate;
>   only an explicit per-session `Off` override still disables a session.
> - **Writes** (`send_text`/`send_keys`) are free between sessions in the same
>   effective sidebar group (valid project override, otherwise manifest
>   project id). Writing across groups consults the app-wide compatibility key
>   `mcp_nonchild_write_access`
>   (`ask` default / `deny` / `allow`): under `ask`, the host checks the
>   persisted `mcp_write_approvals` pair map, and on a miss blocks on
>   `POST /mcp/approve-write` — the app shows an approval alert
>   (`MCPWriteApproval.swift`); Allow remembers the caller→target pair until
>   either session is removed (revocable in Settings ▸ Sessions MCP).
> - `close_session` is **same-group-only**; agents still cannot create
>   sessions (`start_session` was removed). Legacy parent fields are decode-only.
>
> **Security scope (2026-08-14): this is a cooperation policy, not a
> sandbox.** Every local hosted command runs as the same user as Unpeel. The
> on-disk approvals, shared MCP token, and local session sockets therefore do
> not isolate hostile same-UID code; they govern callers that use the supported
> MCP tool path. The filesystem modes protect other local accounts, and the
> token protects the loopback bridge from browser-origin CSRF. Do not present
> Ask or Deny as protection from a malicious repository or command. A hard
> boundary needs an OS-confined session principal and a Host-owned broker.
>
> Authoritative description: the "Built-in MCP Server" section of
> `AGENTS.md` (repo root).

## Summary

Unpeel ships a built-in MCP server — **Unpeel Sessions MCP** — that lets one
agent session inspect and control its **sibling** sessions: read their output,
type prompts into them, answer interactive menus, and start/close sessions. It
is for managing peer sessions in the same project and its worktrees — not a
delegation/subagent mechanism (agents delegate with their provider's own
subagent feature).

Access is governed by a **two-axis grant per session** — a **role** (what it can
do) and a **reach** (which sessions it can touch) — plus a single **app-wide
default** for sessions that have no explicit override.

## Main Files

- MCP server (stdio JSON-RPC, the tools):
  [crates/unpeel-core/src/mcp_host.rs](/Users/tommyvedvik/Dev/unpeel/crates/unpeel-core/src/mcp_host.rs)
- Grant types (`McpRole`, `McpScope`, `McpGrant`) + persisted `AppState`:
  [crates/unpeel-core/src/state.rs](/Users/tommyvedvik/Dev/unpeel/crates/unpeel-core/src/state.rs)
- Manifest + launch (`mcp_enabled`, `mcp_client_registered`):
  [crates/unpeel-core/src/session_host.rs](/Users/tommyvedvik/Dev/unpeel/crates/unpeel-core/src/session_host.rs)
- Per-provider MCP injection:
  [crates/unpeel-core/src/integrations/](/Users/tommyvedvik/Dev/unpeel/crates/unpeel-core/src/integrations/)
  (`claude.rs`, `codex.rs`, `cursor_agent.rs`)
- MCP auth token (`/mcp/*` shared secret):
  [crates/unpeel-core/src/mcp_auth.rs](/Users/tommyvedvik/Dev/unpeel/crates/unpeel-core/src/mcp_auth.rs)
- Native store (grants, default, launch flag, restart recommendations):
  [apps/native/UnpeelNative/Sources/UnpeelNative/UnpeelStore.swift](/Users/tommyvedvik/Dev/unpeel/apps/native/UnpeelNative/Sources/UnpeelNative/UnpeelStore.swift)
- Native models (`McpRole`, `McpGrant`, `SessionAccessLevel`, decoders):
  [apps/native/UnpeelNative/Sources/UnpeelNative/Models.swift](/Users/tommyvedvik/Dev/unpeel/apps/native/UnpeelNative/Sources/UnpeelNative/Models.swift)
- Native lifecycle bridge (`POST /mcp/*`):
  [apps/native/UnpeelNative/Sources/UnpeelNative/MCPBridge.swift](/Users/tommyvedvik/Dev/unpeel/apps/native/UnpeelNative/Sources/UnpeelNative/MCPBridge.swift)
- Sidebar Session Access picker + orchestrator tag:
  [apps/native/UnpeelNative/Sources/UnpeelNative/Views/SidebarView.swift](/Users/tommyvedvik/Dev/unpeel/apps/native/UnpeelNative/Sources/UnpeelNative/Views/SidebarView.swift)
- Settings default picker + deviations list:
  [apps/native/UnpeelNative/Sources/UnpeelNative/Views/SettingsView.swift](/Users/tommyvedvik/Dev/unpeel/apps/native/UnpeelNative/Sources/UnpeelNative/Views/SettingsView.swift)
- Restart-recommendation banner:
  [apps/native/UnpeelNative/Sources/UnpeelNative/Views/TerminalArea.swift](/Users/tommyvedvik/Dev/unpeel/apps/native/UnpeelNative/Sources/UnpeelNative/Views/TerminalArea.swift)

Related: [hosted-sessions.md](/Users/tommyvedvik/Dev/unpeel/docs/feature/hosted-sessions.md),
[hook-system.md](/Users/tommyvedvik/Dev/unpeel/docs/feature/hook-system.md),
[remote-transcript-api.md](/Users/tommyvedvik/Dev/unpeel/docs/feature/remote-transcript-api.md).

## The Access Model

Two orthogonal axes plus a default.

### Role (capability) — `McpRole`

| Role | Read (list/inspect/screen/output/transcript/wait) | `start_session` | Drive (`send_text`/`send_keys`/`close_session`) |
| --- | --- | --- | --- |
| **Off** | – | – | – |
| **Member** | ✅ in reach | ✅ | – (cannot drive any session) |
| **Orchestrator** | ✅ in reach | ✅ | ✅ any session in reach |

Driving is a pure Orchestrator capability (`permits_mutate` = `role ==
Orchestrator`). A project-reach Orchestrator drives its **siblings and
worktrees** (its own project tree); a global-reach one drives any session.

> **No lineage/child model.** There used to be a "Member can drive sessions it
> started" rule keyed on `parent_session_id`. It was removed (2026-07-01): the
> parent→child link broke whenever either session restarted (restart mints a new
> id), and provider-native subagents are the right tool for delegation. Unpeel
> Sessions MCP is for managing **peer** sessions, not a parent/child agent tree.
> `start_session` opens a sibling session; for delegated subtasks, an agent
> should use its provider's built-in subagent feature (Claude Task/subagents,
> Codex, …) unless the user explicitly asks for separate Unpeel sessions. The
> server `instructions` string steers agents this way.

### Reach (blast radius) — `McpScope`

| Reach | Sees / controls |
| --- | --- |
| **project** | the caller's own project tree — same `project_id`, or any worktree in the same group (worktree siblings count as one project) |
| **global** | every session, in every project |

Reach is evaluated per call by `mcp_scope_permits` / `projects_in_same_tree`.

### The four user-facing levels — `SessionAccessLevel`

The UI collapses role × reach into one choice:

| Level | role | reach |
| --- | --- | --- |
| **No access** | Off | – |
| **Read & create** | Member | project |
| **Control this project** | Orchestrator | project |
| **Control all projects** | Orchestrator | global |

### The app-wide default — `mcp_default_access`

A single grant applied to every session that has **no explicit override**.
Stored as `mcp_default_access` in `app-state.json` (default: Member at project
reach). The user can raise or lower it app-wide from **Settings ▸ Unpeel
Sessions MCP ▸ Default for every session** (all four levels are offered).

- Setting the default to **No access** is the locked-down, opt-in posture: new
  sessions launch with no MCP client and must be raised per session.
- Setting it to a **Control** level makes every session an orchestrator by
  default; the per-session list is then used to dial specific sessions *down*.

### Override map — `mcp_orchestrators`

A top-level map in `app-state.json`, keyed by session id → `McpGrant`
`{ "role", "reach" }`. **It only stores deviations from the default**: setting a
session to the current default removes its entry, and changing the default
prunes entries that become redundant. That is what keeps the Settings "Custom
access" list short — it lists exactly the override map.

> **Back-compat:** the JSON key is `mcp_orchestrators` (historical name), and a
> legacy bare-string value (`"project"` / `"global"`) still decodes — to an
> Orchestrator grant at that reach.

## Enforcement — Two Points

Access is enforced in two places, and you need both for correct behavior.

### 1. Launch (whether the client is registered at all)

The MCP client only registers with a provider CLI at process start. The native
app computes `mcp_enabled` for each launch:

```
mcp_enabled = (override?.level ?? default).level != .noAccess  &&  !projectBlocked
```

The host records this into the manifest as `mcp_client_registered`. Providers
without an MCP integration ignore `mcp_enabled` (harmless no-op).

Per-provider injection (all resolve to the same `unpeel-host __mcp__` server):

| CLI | Mechanism |
| --- | --- |
| **Claude** | appends `--mcp-config ~/.unpeel/mcp/claude-mcp.json` (additive; skipped if the user already passes `--mcp-config`). `claude::startup_command`. |
| **Codex** | the Unpeel codex wrapper (`~/.unpeel/hooks/bin/codex`) injects `-c mcp_servers.unpeel-sessions.*` config overrides when `UNPEEL_MCP_BIN` is set; session id is passed via explicit `env` because Codex starts MCP servers with a minimal environment. `codex::configure_host_command`. |
| **Cursor** | registers the server in `~/.cursor/mcp.json` (`install_cursor_hooks`) and adds `--approve-mcps` to auto-approve it. `cursor_agent::startup_command`. |
| **Kimi** | Current Kimi Code removed per-launch MCP flags, so Unpeel merges environment-gated entries into `~/.kimi-code/mcp.json`; the gates expose tools only inside the granted hosted session. Legacy Kimi still receives `~/.unpeel/mcp/kimi-mcp.json` through repeatable `--mcp-config-file`, including an existing `~/.kimi/mcp.json` when required. `kimi::startup_command`, `mcp_gate`. |
| gemini, amp, pi, opencode, grok, copilot | **no MCP path** — the access grant is meaningless for them, so the UI hides Session Access. |

### 2. Per tool call (what a registered client may actually do)

The host re-reads `app-state.json` on **every** tool call (`load_security` in
`mcp_host.rs`), so role/reach/default changes apply live to already-connected
sessions — no relaunch:

- `effective_grant(caller)` — unknown caller → `Off`; known caller → its override
  or the app-wide default.
- `caller_refusal_reason` — refuses the whole call if the caller is unknown or
  `Off`.
- Reads → `require_session(_, Read)` (reach only).
- Mutations (`send_text`, `send_keys`, `close_session`) → reach **plus**
  `permits_mutate(caller, target)` = `role == Orchestrator || target.parent ==
  caller.id`.
- `start_session` → any Member, but a project-reach caller may only launch into
  its own tree.
- A session may never write into or close **itself** (`"self": true` is refused).

## Changing Access & Restarts

`UnpeelStore.setSessionAccess` / `setDefaultAccess` persist the grant and rescan.
**They do not auto-restart.** Whether a restart is needed:

| Transition | Restart? | Why |
| --- | --- | --- |
| Member ⇄ Orchestrator, reach project ⇄ global | **No** | The registered tool list is identical; the host re-reads the grant per call. Pure live change. |
| → No access | **No** | The per-call gate refuses everything immediately; the dormant client just disappears on the next natural restart. |
| No access → a level that needs the client | **Yes** | The client only registers at launch; it can't appear without a relaunch. |
| Changing the **default** | **No (never mass-restarts)** | Live for reads/drives via the per-call gate. |

Restarts are surfaced through the existing **restart-recommendation banner**
(`RestartRecommendedBar`), never by yanking the terminal. The recommendation is
derived per session in `rescan()` from `restartRecommendation(for:wantsMcpClient:)`:

- It compares the session's grant to `manifest.mcp_client_registered`.
- It is intentionally scoped to **explicitly elevated** sessions
  (`mcpOrchestrators[id]?.role == .orchestrator`) that launched without the
  client — so flipping the model to default-on does **not** put a banner on
  every existing session.
- Token `mcp-access:on`, message "Restart to apply this session's Sessions MCP
  access." Host-protocol recommendations take priority.
- The banner's Restart button calls `restartSession`, which resumes the
  conversation and carries title/pin/worktree/grant to the new session id; the
  relaunch sets `mcp_client_registered`, clearing the token.

See the "Restart Recommendation API" section in
[AGENTS.md](/Users/tommyvedvik/Dev/unpeel/AGENTS.md) for the shared banner API.

## Tool Surface

12 tools, exposed over MCP stdio. There is intentionally **no `restart` tool**
(restart is a native-only maintenance action; the public surface is start/close)
and **no `team_status`** (removed with the child model).

```
list_sessions  inspect_session  read_screen  read_output  read_transcript
wait_for_text  wait_for_status  send_text  send_keys
list_presets  start_session  close_session
```

- Reads return ANSI-stripped / rendered output; `read_transcript` uses the
  shared provider transcript API (see remote-transcript-api.md).
- `send_text` is the proven recipe (bracketed paste + settle + double Enter);
  follow it with `wait_for_status` or `wait_for_text` rather than polling.
- Cross-group `send_text` deliveries are prefixed with a provenance
  header — `[message from id:<sender>, channel: terminal]` — so the receiving
  agent knows who is talking and can reply to that id; same-group traffic is
  delivered verbatim (see sessions-mcp-channels.md for the channel direction).
- `start_session` opens a sibling session (preset or command, optionally in a
  worktree) with an optional `label` and `start_message`. It does **not** stamp
  a parent/child relationship — for delegated subtasks use the provider's native
  subagent feature.

### Lifecycle bridge (`/mcp/*`)

Start/close/restart need the app (the session must be persisted,
activity-registered, and pushed to the sidebar), so those route over HTTP to the
native bridge (`MCPBridge.swift`):

- `POST /mcp/list-presets | start-session | restart-session | close-session` on
  the hook-server port.
- `restart-session` is a native maintenance endpoint (same `restartSession` path
  as the UI); it is **not** exposed as an MCP tool.
- Unlike hook routes, `/mcp/*` requires the `x-unpeel-auth` header matching
  `~/.unpeel/mcp/auth-token` (these endpoints can launch arbitrary commands and
  localhost is reachable by browser CSRF).

## `app-state.json` Shape

```jsonc
{
  // app-wide default for sessions without an override (absent ⇒ Member/project)
  "mcp_default_access": { "role": "member", "reach": "project" },

  // deviations only — keyed by session id
  "mcp_orchestrators": {
    "15dec879-…": { "role": "orchestrator", "reach": "global" },  // Control all projects
    "a1b2c3d4-…": { "role": "off",          "reach": "project" }  // No access
    // a legacy value "project" / "global" also decodes (→ Orchestrator)
  }
}
```

The native app writes these via a field-preserving read-modify-write
(`mutateAppStateJSON`) so it never drops keys it doesn't model. The MCP host
reads the same file directly — it does not need the app running, only the
session hosts.

## Caller Identity

The MCP host identifies the caller from `UNPEEL_SESSION_ID` in its inherited
environment (exported into each launched session by the host when `hook_port` is
set). A call from an unknown session id is refused.

## Debugging

- `mcp-host` lines in `~/.unpeel/hooks/trace.log`.
- Drive the host manually:
  `printf '...JSON-RPC...' | UNPEEL_SESSION_ID=<id> unpeel-host __mcp__`
  (initialize → `notifications/initialized` → `tools/call`).
- Inspect a grant: `~/.unpeel/app-state.json` → `mcp_default_access` /
  `mcp_orchestrators`.
- Confirm the client was injected: the session's manifest
  (`~/.unpeel/app-sessions/<id>/manifest.json`) `mcp_client_registered`, and for
  Claude the launched command's `--mcp-config`.

## Tests

- `cargo test --manifest-path crates/Cargo.toml` — grant parse/back-compat
  (`state.rs`), role + reach + default gate (`mcp_host.rs`).
- `swift test --package-path apps/native/UnpeelNative --filter AppStateFileTests`
  — grant + default decode.
- `swift build` in `apps/native/UnpeelNative`.
