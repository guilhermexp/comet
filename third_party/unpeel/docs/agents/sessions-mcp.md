<!-- Split out of the repo-root AGENTS.md (2026-08-05). The root AGENTS.md holds the map, hard rules, and invariants; this file is the full detail for its topic. -->

## Built-in MCP Server (Session Control)

Unpeel ships Unpeel Sessions MCP so an agent session can inspect and control other sessions (read their output, type prompts into them, answer interactive menus). Full walkthrough: `docs/feature/sessions-mcp.md`. Long-term direction: messaging becomes channel-based — terminal↔terminal is the default channel, later Slack↔terminal — see `docs/feature/sessions-mcp-channels.md`. Cross-group `send_text` deliveries are prefixed with `[message from id:<sender>, channel: terminal]`; same-group traffic stays verbatim. Don't hard-wire new messaging features to "the other end is a PTY", and route inter-session text through the `deliver_text_to_terminal` choke point in `mcp_host.rs`.

**Experimental (2026-07-09):** Sessions MCP is gated behind Settings ▸ Experimental like worktrees (`ExperimentalFeature.sessionsMcp` in `FeatureFlags.swift`, env escape hatch `UNPEEL_DEV_SESSIONS_MCP=1`). The flag gates the Settings ▸ Sessions MCP tab and whether new sessions launch with the MCP client injected (`mcp_enabled` in the spawn payload). Existing live sessions keep their launch-time domain set until the agent starts in a newly configured terminal; the transcript settings are **not** part of this gate (they live in the general Settings ▸ Transcripts tab and also serve Copy transcript on desktop and phone).

> **Unified surface (2026-07-18, renamed 2026-07-25):** `unpeel-host __mcp__`
> is now the single
> **`unpeel`** MCP server for all built-in capabilities (named `unpeel-mcp`
> until 2026-07-25; the old name lives on only as pruned legacy config
> entries and in the pre-rename config *file names*, which are kept so
> restart commands recorded by older sessions keep resolving): **one action-enum
> tool per domain** — `sessions` and `browser` today, `computer`/`device`
> planned (`docs/feature/unified-mcp.md`) — instead of one server per domain
> with a dozen tools each. Schemas are terse (~1.5k tokens for both domains,
> enforced by a byte-ceiling test in `mcp_host.rs`); full per-action docs load
> lazily via `{"action":"help"}`. A domain is advertised only if the caller's
> saved domain grant (`mcp_enabled` / `browser_mcp_enabled` /
> `computer_mcp_enabled`) is set — a session launched without a domain never
> pays its context cost — and
> per-call gates still apply live. Legacy per-tool names (`browser_*`) and the
> per-domain `__mcp_gate__ <domain>` argv keep resolving onto this same server,
> bounded to their one domain, for sessions launched pre-unification.
> The separate `*_client_registered` fields say whether Unpeel injected the
> provider configuration automatically. They remain false for a blank shell;
> a CLI configured manually with `unpeel-host __mcp__` still receives only the
> saved grants.
> Injection is **one config per provider** (claude `claude-unpeel-mcp.json`,
> codex wrapper `mcp_servers.unpeel` via `UNPEEL_MCP_BIN`, legacy kimi
> `kimi-unpeel-mcp.json`, Kimi Code `__mcp_gate__ unified`, cursor/cline a
> single `unpeel` entry, kiro's combined server delegates); the env var /
> config is present when *any* domain is enabled. Persistent configs
> (cursor `~/.cursor/mcp.json`, Kimi Code `~/.kimi-code/mcp.json`, kiro
> `settings/mcp.json`) prune the managed pre-rename `unpeel-mcp` entry the
> same way the unification pruned `unpeel-sessions`/`unpeel-browser`.
>
> **Computer domain (development-only containment as of 2026-08-14; engine
> swapped 2026-07-22):** the
> `computer` action tool (`crates/unpeel-core/src/computer_mcp.rs`,
> **cua-driver** engine — see `docs/feature/computer-mcp.md`) gives a session
> **background** control of the user's REAL apps: `launch` → pid + windows,
> `see` → accessibility tree (`[N]` element indices) + screenshot artifact,
> then click/type/set_value by element index — no focus steal, the user's
> cursor never moves (a per-session overlay cursor glides instead).
> Desktop-wide scope needs an explicit `escalate` (cua's one-way window→
> desktop ladder; each Unpeel session is cua session `unpeel-<id>`). Release
> builds hide the feature, force its launch flag off, stop stale daemon state,
> and omit cua-driver: the unrestricted TCC-bearing socket is not isolated
> from same-UID hosted code. Development builds remain gated by
> `ExperimentalFeature.computerUse` (`UNPEEL_DEV_COMPUTER_USE=1`),
> `computer_default_access` (`off`/`ask` default/`allow` in state.rs), and
> under `ask` a one-time per-session approval alert (`/mcp/approve-computer`,
> `MCPComputerApproval.swift`; remembered in `computer_approvals`,
> pruned/carried like write approvals). The **native app owns the engine
> daemon** (`ComputerEngineManager.swift` spawns `cua-driver serve --embedded
> --socket ~/.unpeel/computer/daemon.sock` as a direct child so TCC
> attributes to Unpeel.app — never spawn it from a session host or via
> `open`); `computer_mcp.rs` makes one-shot `cua-driver call … --socket`
> invocations against it. Grants are probed/requested natively
> (`ComputerPermissions.swift`); the daemon restarts on grant changes.
> Captures land in `artifacts/computer/screenshots/` (phone gallery kind
> `computer`); engine resolution `UNPEEL_CUA_DRIVER_BIN` → bundled sibling →
> `~/.unpeel/computer/bin` → PATH. Session cleanup rides
> `__computer_cleanup__ <id>` next to `__browser_cleanup__`. Not yet: live
> validation against an installed engine, vendored pinned binary,
> `verify-computer.sh`. Its Ask prompt is a cooperative agent control, not a
> sandbox boundary; see `docs/feature/computer-mcp.md`.

- Server: `crates/unpeel-core/src/mcp_host.rs`, run as `unpeel-host __mcp__`. Speaks MCP JSON-RPC over stdio; hand-rolled, no SDK dependency.
- It talks directly to per-session artifacts (`manifest.json`, `output.bin`, `session.sock`) under `~/.unpeel/app-sessions/`; it does not need the app running, only the session hosts.
- Caller identity comes from `UNPEEL_SESSION_ID` in the inherited env. Writing into the calling session's own terminal is refused.
- MCP transcript reads use the shared provider transcript API in
  `crates/unpeel-core/src/transcripts/mod.rs`, so adapter/parser changes affect MCP
  and remote clients together.

Sessions actions (19, on the single `sessions` tool; legacy names in parentheses where they differ): `current` (`get_current_session`), `list` (`list_sessions`), `inspect` (`inspect_session`, the preferred first read: compact status + screen/transcript tails), `read_screen` (rendered viewport via `ViewportSnapshot` with `cols=0/rows=0` = "keep current size"; hosts from older builds can't serve this and get a clear error), `read_output` (ANSI-stripped tail of `output.bin`), `read_transcript`, `wait_for_text` (block until a substring appears on the rendered screen, case-insensitive by default, default 30s / max 120s timeout; reports the final screen tail on timeout — the reliable follow-up to `send_text`/`send_keys` instead of polling `read_screen`), `wait_for_status`, `send_text` (bracketed paste + settle delay + double Enter, the proven recipe), `send_keys` (named keys with pacing, e.g. `["down","enter"]` to answer menus), `list_group`, `wait_for_group`, `summarize_group`, `report_to_group`, `add_to_gallery` (copy a local PNG/JPEG/GIF/WebP, max 32 MiB, into the caller's own `artifacts/uploads` gallery directory; relative paths resolve from its manifest cwd), `list_presets`, `create_worktree`/`list_worktrees` (gated on `AppState.mcp_worktree_access`, default off, Settings ▸ Sessions use toggle shown while worktrees are enabled; bridge routes `/mcp/create-worktree|list-worktrees` reuse `sessionLaunchTarget`), `close` (`close_session`). Session creation is user-only: stale `start_session` calls are refused, and `create_worktree` prepares a checkout without launching a session. Pre-group helper names (`list_children`, `wait_for_children`, `summarize_children`, `report_to_parent`) remain unadvertised decode aliases for already-running clients and resolve to the group helpers.

Session lifecycle (close/presets — and the write-approval prompt) cannot run host-side — so those tools call the app over HTTP:

- Bridge: `MCPBridge.swift` (native), routed as authenticated `POST /mcp/*` calls on the hook-server port. Public Sessions actions use `list-presets`, `create-worktree`, `list-worktrees`, `close-session`, and `approve-write`; `start-session` is reserved for user/controller launches and ignores legacy lineage fields, while `restart-session` is a native-maintenance route. Neither is a public agent action. `approve-write` presents the cross-group write approval and replies asynchronously when the user answers (HookServer's per-request wait ceiling is 150s; the host client reads with a ~130s timeout). The MCP host tries the session's launch-time `UNPEEL_APP_PORT`, then the `~/.unpeel/app-ports` registry (newest first).
- Auth: unlike hook routes, `/mcp/*` requires the `x-unpeel-auth` header matching `~/.unpeel/mcp/auth-token` (0600, created at hook-server start by `mcp_auth.rs` / the native `MCPAuth`) — the endpoints can launch arbitrary commands, and localhost is reachable by browser CSRF.
- Worktree creation and close map onto the same native paths as their UI verbs. The MCP host defaults `project_id` to the calling session's project and refuses to close its own session.

> **Security scope (2026-08-14): these are cooperative controls, not
> same-UID isolation.** Hosted commands run as the user's account and are not
> sandboxed by Unpeel. The `0700` Unpeel home and `0600` MCP token protect
> against other local users and browser-origin CSRF; they do not stop code in
> a hosted session from reading same-user state or discovering local sockets.
> Consequently the same-group and Ask/Deny rules below govern agents that use
> the supported MCP surface, but must never be described as a security boundary
> against malicious shell code. A hard boundary requires a Host-owned broker
> plus OS-enforced session confinement.

Cooperative access policy — **open reads, same-group writes, approval for
cross-group writes** (reworked 2026-08-10):

- **Reads are open across ALL sessions.** Any enabled caller can `list_sessions`/`inspect_session`/read any session in any project (`McpSecurity::permits_manifest` = caller known and not internally `Off`). The old project/worktree reach machinery was removed from the gate; `McpScope`/`mcp_default_access` survive only as decode-tolerant legacy fields (an explicit per-session `Off` override in `mcp_orchestrators` still disables a session's tools entirely).
- **Same-group writes are free.** A session may always `send_text`/`send_keys` to another session in its effective sidebar group. The effective group is a valid `project-override.json` target, otherwise the manifest `project_id`; a root project, every plain group, and every worktree are distinct. Moving a session changes the boundary immediately. `close_session` is same-group-only and never falls through to approval.
- **Cross-group writes go through the app-wide write policy** stored under the compatibility key `AppState.mcp_nonchild_write_access` (`ask` default / `deny` / `allow`, `McpNonChildWriteAccess` in `state.rs`), re-read per call so changes apply live. Under `ask`, `require_session(_, Write)` first checks the persisted pair map `AppState.mcp_write_approvals` (`caller id → [target ids]`, directional); on a miss it POSTs `/mcp/approve-write` to the app with a 130s read timeout (`request_write_approval`) and the user answers the approval prompt — Allow persists the pair, Deny fails the tool call with a clear "don't retry" message. Prompts are FIFO and identical pairs coalesce; the exited-target check runs before the prompt so a dead session never asks.
- **Legacy lineage is decode-only.** `parent_session_id`, `session_parents`, and the remote protocol's `parentSessionID` remain tolerated for older manifests/controllers, but current hosts never write or enforce them and current clients render sessions flat.
- **Unified approval prompts, answerable from controllers (2026-07-25):** the three ask-mode routes (`/mcp/approve-write|browser|computer`) share one pending queue (`PendingMcpApproval` in `MCPApprovalCenter.swift`; the per-route handlers keep their fast paths). The desktop surface is a **floating non-modal panel** (`MCPApprovalPanel.swift`) — never `NSAlert.runModal()`, which (fired from the `Task { @MainActor }` bridge dispatch with no key window) nested a modal run loop inside a main-actor job and stalled every queued main-actor job, including the mobile server's bootstrap hop — paired phones dropped to "Connection lost" whenever a prompt appeared unattended. Pending prompts also ride the phone bootstrap (`pendingApprovals` on `RemoteBootstrapSnapshot`, Mac-resolved title/body so new kinds need no phone update) and are answered via `POST /mobile/approvals/answer` (409 = already answered elsewhere; first answer wins, both surfaces race on `UnpeelStore.answerMcpApproval(id:approved:)`). New prompts push to paired phones (`SessionPushKind.approval`, no macOS banner — the panel is that surface); the iOS prompt is a root ZStack overlay (`ApprovalPromptOverlay` in `UnpeelIOSRootView.swift`), not a `.sheet`. The computer-permissions TCC nudge (`ComputerPermissions.swift`) uses the same floating-panel controller for the same no-runModal reason.
- **Approval lifecycle:** pairs live in `~/.unpeel/app-state.json`; an in-place Resume Agent after the managed runtime returns to its shell keeps the same Session id and therefore needs no migration. Replacement Resume/handoff paths snapshot the map before `pruneNativeState` and re-add every pair under the new Session id (both directions), using the same read-before-prune discipline as the carried access grant.
- **Launch injection is unchanged:** `SessionHostLaunch.mcp_enabled` still decides both the saved Sessions-domain grant and whether a managed provider gets automatic configuration (Claude `--mcp-config`, Codex `-c mcp_servers.*`, Cursor `~/.cursor/mcp.json` + `--approve-mcps`, current Kimi's environment gate in persistent `~/.kimi-code/mcp.json`, legacy Kimi repeatable `--mcp-config-file`, Cline per-session `CLINE_MCP_SETTINGS_PATH`; other CLIs ignore it). The manifest records those as distinct `mcp_enabled` and `mcp_client_registered` facts.
- **Native UI:** Settings ▸ Sessions use explains that same-group control is implicit, then offers the cross-group write policy picker, the Browser-screenshot auto-gallery toggle (`mcp_auto_add_browser_screenshots`, default true), and an "Approved sessions" list with per-pair Revoke. When auto-add is off, ordinary Browser MCP screenshots stay under unlisted `artifacts/browser/captures`; an agent can publish a selected file with `add_to_gallery`. Explicit phone screenshot requests pass `gallery=true` and still appear. All changes apply live; nothing here drives a restart banner.

> **Removed (2026-06-22):** the per-project MCP *block* feature (`mcp_blocked_projects`, `Project.mcp_blocked`, the Settings "Block individual projects" section, host/bridge block gates) is gone. The native `AppStateFile`/`Project` decoders still tolerate the old `mcp_blocked*` keys for backward-compatible reads, but nothing writes or enforces them.

Auto-registration per provider:

- Claude: `install_claude_hooks` writes `~/.unpeel/mcp/claude-unpeel-mcp.json` (rewritten each launch so the exe path — `unpeel-host` — stays current; the legacy `claude-mcp.json` is still rewritten for pre-unification live sessions); `claude::startup_command` appends one `--mcp-config <path>` when any domain is enabled (skipped if the user already passes `--mcp-config`).
- Codex: the wrapper at `~/.unpeel/hooks/bin/codex` injects `-c mcp_servers.unpeel.*` overrides when `UNPEEL_MCP_BIN` is set (exported by `codex::configure_host_command` when any domain is enabled, pointing at `unpeel-host`); session identity is passed via explicit `env` because Codex spawns MCP servers with a minimal environment.
- Kimi: `install_kimi_hooks` supports both generations. Current Kimi Code gets one merged `~/.kimi-code/mcp.json` entry `unpeel` pointing at `unpeel-host __mcp_gate__ unified` (enabled when either grant env var is set; managed legacy `unpeel-mcp`/`unpeel-sessions`/`unpeel-browser` gate entries are pruned); `kimi::startup_command` probes `kimi --help` and uses the old repeatable `--mcp-config-file` injection (now one `kimi-unpeel-mcp.json`) only for legacy Kimi, preserving its implicit `~/.kimi/mcp.json` behavior.
- Cline: `cline::configure_host_command` copies the current user MCP settings
  into `app-sessions/<id>/cline-mcp-settings.json`, adds only the servers
  granted to that launch, and selects the copy with
  `CLINE_MCP_SETTINGS_PATH`. Concurrent sessions can have different grants and
  the user's global file stays untouched.

Debugging: `mcp-host` lines in `~/.unpeel/hooks/trace.log`. Test with `printf '...' | unpeel-host __mcp__`.
