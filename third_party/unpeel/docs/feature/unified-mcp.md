# Unified MCP: one server, one tool per domain

Status: **Phases 1–3 shipped 2026-07-18** (sessions + browser + computer
domains live on the unified server — computer experimental, see the status
note in `computer-mcp.md`; device still planned and blocked on the Node
decision). Three-domain advertised surface measures ~9.6 KB (~2.4k tokens),
per-domain and total ceilings enforced in `mcp_host.rs` tests. Consolidates Unpeel's
built-in MCP surface — Sessions, Browser, the planned Computer Use
(`docs/feature/computer-mcp.md`), and a new Device domain — into **one MCP
server** exposing **one action-enum tool per domain**, to stop per-request
context cost growing linearly with every capability we add.

Implementation notes (what shipped, where it deviates from the plan below):

- Measured result: both domains advertise in **5,954 bytes (~1.5k tokens)** vs
  15,786 bytes (~3.9k) for the two old servers; guarded by
  `advertised_schema_stays_within_the_context_budget` (8 KB ceiling) in
  `mcp_host.rs`.
- The legacy per-tool definitions were kept (unadvertised) as the doc source
  for `action:"help"` and as the dispatch contract for stale clients — legacy
  tool names still execute.
- Domain advertising reads the caller manifest (`mcp_client_registered` /
  `browser_client_registered`); unknown callers (raw-pipe testing) see all
  domains, and per-call gates enforce access either way. Both domains also got
  a manifest-flag check in their call gate, since the unified config is
  injected when *any* domain is enabled.
- Three consumers the plan missed, all consolidated too: **Kimi Code** and
  **Kiro** use the provider-neutral `__mcp_gate__ unified` entry (with managed
  legacy entries pruned), while **Cline** writes a single `unpeel` entry into
  its per-session settings copy.
- `verify-browser.sh` drives the unified `__mcp__` surface (the `browser` tool)
  since the standalone arm was retired.

## Problem

Every MCP tool schema rides in **every request** of every session. Measured
from the shipped binaries (`tools/list` JSON, ~4 bytes/token):

| Server | Tools | Schema size | ~Tokens/request |
| --- | --- | --- | --- |
| unpeel-sessions (`__mcp__`) | 16 | 10,641 B | ~2,700 |
| unpeel-browser (`__browser_mcp__`) | 13 | 5,145 B | ~1,300 |
| computer (planned, est.) | 12 | — | ~1,500 |
| device (planned, est.) | ~9 | — | ~1,300 |

Plus a per-server instructions block (a few hundred tokens each) and, on the
app side, a per-server injection (Claude: one `--mcp-config` each; Codex: one
wrapper block each; Kimi: one `--mcp-config-file` each; Cursor: one merged
entry each). Four servers ≈ **7–9k tokens in every request, ~50 tools**, and
each future domain adds another server × another provider-injection row.

## Design

Prior art: Executor (executor.sh) collapses hundreds of tools into a single
execute tool with search/describe/call; Anthropic's own computer-use API is
**one tool with an `action` enum**. We take the middle point: capabilities
stay *advertised* (the model always knows it has a browser/screen/devices —
critical for the weaker CLIs Unpeel supports), but schemas collapse.

### One server

- `unpeel-host __mcp__` becomes the **unified server**, MCP server name
  `unpeel` (so CLI UIs read "calling unpeel"; named `unpeel-mcp` from
  2026-07-18 until the 2026-07-25 rename — persistent provider configs prune
  the old managed entry, and the pre-rename config *file names* like
  `claude-unpeel-mcp.json` are kept so recorded launch commands still
  resolve). `mcp_host.rs` keeps
  transport/identity/security and becomes a dispatcher over domain modules:
  - `sessions` — the Sessions actions, folded into one tool (impl stays in
    `mcp_host.rs` or moves to `mcp/sessions.rs`)
  - `browser` — shared impl refactored out of `browser_mcp.rs`
  - `computer` — per `docs/feature/computer-mcp.md`, landing directly as a
    domain module (never as a fourth standalone server)
  - `device` — new (engine story below)
- One merged instructions block replaces the per-server ones (and gets
  shorter than their sum — the read/write/approval rules are shared).
- Caller identity (`UNPEEL_SESSION_ID`), `x-unpeel-auth` bridge auth
  (`mcp_auth.rs`), and the per-call live gates all carry over unchanged —
  gates were always per-call, never per-server.

### One tool per domain, `action` enum

```
sessions(action: current|list|inspect|read_screen|read_output|read_transcript|
                 wait_for_text|wait_for_status|send_text|send_keys|
                 list_group|wait_for_group|summarize_group|
                 report_to_group|add_to_gallery|list_presets|close|help, ...)
browser (action: open|snapshot|click|fill|type|press|get|screenshot|wait|
                 scroll|console|close|context|help, ...)
computer(action: apps|launch|quit|front|windows|see|screenshot|desktop|click|
                 type|set_value|press|hotkey|scroll|drag|move_cursor|escalate|
                 context|help, ...)   # cua-driver engine, 2026-07-22
device  (action: apps|open|snapshot|tap|fill|scroll|screenshot|close|context|
                 help, ...)
```

- Params: a small shared set of optional fields per tool (`target`, `text`,
  `url`, `session_id`, `keys`, …) with one-line descriptions noting which
  actions use them. No `oneOf`-per-action schema gymnastics — that would
  rebuild the bloat we're removing.
- **`action: "help"`** (optionally `help_for: "<action>"`) returns full
  per-action docs, required params, and examples. This is Executor's
  `describe` step without a generic call gateway: detail is loaded only when
  needed, and only into the sessions that need it.
- Tool descriptions stay terse (2–3 sentences + "call help for details").

**Budget: ≤ ~2k tokens for the full four-domain surface** (vs ~7–9k), and a
new domain costs one tool (~400–500 tokens), not another server. Enforced by
a regression test: `tools/list` JSON must stay under a byte ceiling
(`cargo test` asserts, e.g., < 8 KB total).

Why not the full Executor gateway (`search`/`describe`/`call`, ~500 tokens
flat): every action becomes a two-step dance, and the weaker provider CLIs
(kimi, grok, pi) fumble generic indirection far more than concrete tools.
Revisit only if the domain count grows past what ~2k tokens covers.

### Domain advertising is dynamic

`tools/list` is computed per session:

- A domain appears only if its launch flag was set for this session
  (manifest `*_client_registered`) — e.g. computer absent on macOS < 15,
  device absent when no engine is installed. An absent domain costs **zero**
  context.
- Live revokes still work through the per-call gate (Settings ▸ Off fails
  calls immediately, exactly like today's browser gate); advertising catches
  up at the session's next restart. No behavior change from the shipped
  model.

### Launch & injection (the big simplification)

- `SessionHostLaunch` keeps per-domain booleans (`mcp_enabled`,
  `browser_mcp_enabled`, `computer_mcp_enabled`, `device_mcp_enabled`),
  recorded in the manifest as today (`mcp_client_registered`,
  `browser_client_registered`, + two new).
- But providers get **one injection** when *any* domain is enabled:
  - Claude: single `--mcp-config ~/.unpeel/mcp/claude-unpeel-mcp.json`
    (server `unpeel`, args `["__mcp__"]`), written by `install_claude_hooks`;
    replaces the claude-mcp / claude-browser-mcp pair (files kept for legacy
    live sessions, no longer referenced by new launches).
  - Codex: single `UNPEEL_MCP_BIN` export + one wrapper block
    (`-c mcp_servers.unpeel.*`); the `unpeel-browser` block retires from the
    wrapper template.
  - Kimi: single `~/.unpeel/mcp/kimi-unpeel-mcp.json` via one
    `--mcp-config-file` (implicit `~/.kimi/mcp.json` append unchanged).
  - Cursor: `write_cursor_mcp_config` merges one `unpeel` entry.
- The unified server learns this session's enabled domains from its manifest
  (it already reads manifests for identity/security).

### Compatibility

- The standalone `__browser_mcp__` argv arm was **retired** once the deprecation
  window closed: `browser_mcp.rs` is now purely the implementation library
  (`tool_definitions` / `run_tool`) behind the unified host's `browser` domain,
  and the per-domain `__mcp_gate__ browser` argv resolves onto the same unified
  server bounded to that one domain. `__browser_cleanup__` stays — it is a
  one-shot cleanup command, not a server. Legacy per-domain config files are no
  longer written; managed stale entries are pruned on install.
- Sessions MCP is still experimental-gated (`ExperimentalFeature.sessionsMcp`),
  so renaming its legacy tool surface to `sessions` actions has no public
  contract to break. `docs/feature/sessions-mcp.md` and the
  `creation_disabled_message` behavior carry over as actions.
- No `host_protocol_version` bump: live hosts are untouched; this is all
  client-injection + server-binary behavior picked up on natural restarts.

## Device domain (agent-device)

Engine: **agent-device** (github.com/callstack/agent-device, MIT, v0.19.x) —
agent-browser's sibling: token-efficient snapshots + semantic refs for iOS
Simulator/device (XCTest), Android emulator/device (adb), tvOS, macOS, Linux.
Vocabulary (`open`/`snapshot`/`tap`/`fill`/`screenshot`) maps 1:1 onto the
`device` action tool; screenshots follow the same artifact contract
(`artifacts/device/screenshots/`, path-returning tools, gallery kind added in
`MobileRemoteServer.swift` `listedArtifactKinds`/`artifactKindDir`).

**⚠️ Open decision — the Node conflict.** agent-device is TypeScript on
Node 22+, with **no native mode** (unlike agent-browser). Unpeel never ships
or requires Node, and the browser deep-check doc records "never
detect/enable Node" for the browser engine. Options:

1. **Optional, never-bundled engine** (recommended): the device domain
   resolves its engine from `UNPEEL_DEVICE_BIN` → `~/.unpeel/device/bin` →
   PATH only; Unpeel never ships it. Users who want device control (mostly
   app developers, who have Node already) `npm i -g agent-device`; for
   everyone else the domain is absent from `tools/list` at zero cost. Keeps
   "never ship or require Node" literally true; softens the browser-era
   "never detect" stance to "never bundle" — needs an explicit product
   sign-off before building.
2. **Upstream a native mode** (like agent-browser's `--native`): the right
   long-term fix, but an XCTest bridge + adb wrapper in Rust is a large
   contribution; device support would wait indefinitely.
3. **Native thin slice**: shell out to `adb` and `xcrun simctl` directly
   (standalone binaries, no Node) — launch/install/screenshot everywhere,
   input injection on Android; but no semantic UI snapshots on iOS without
   the XCTest runner. A degraded, Node-free baseline that could ship first
   and be superseded by 1 or 2.

Access model: `device_default_access` (`Off`/`On`, default **On** like the
browser — simulators/emulators are isolated surfaces; a *physical* device
action is arguably more personal, revisit before stable). Settings ▸ Device
panel mirrors the browser panel (engine probe incl. "requires agent-device"
state, access picker).

## What changes where (choke-point map)

| Layer | Change |
| --- | --- |
| `crates/unpeel-core/src/mcp_host.rs` | becomes unified dispatcher; sessions tools → actions; per-session domain advertising; `help` action machinery |
| `crates/unpeel-core/src/browser_mcp.rs` | impl extracted to shared domain module; legacy `run_stdio` kept as shim |
| `crates/unpeel-core/src/computer_mcp.rs` (planned) | lands as domain module `mcp/computer.rs` instead of standalone server |
| new `mcp/device.rs` | device domain + engine probe |
| `crates/unpeel-host/src/main.rs` | `__mcp__` = unified; legacy browser arms kept |
| `state.rs` | + `computer_*` fields (per computer-mcp.md), + `device_default_access` |
| `session_host.rs` | + `computer_mcp_enabled`/`device_mcp_enabled` on launch + manifest flags |
| `integrations/claude.rs`, `codex.rs`, `kimi.rs`, `hook_assets.rs` | collapse to single-config injection; new config writers; wrapper template updated |
| `MCPBridge.swift` / `HookServer.swift` | unchanged routes + `/mcp/approve-computer` (per computer-mcp.md) |
| `SettingsView.swift` | + Computer and Device panels; Sessions MCP panel unchanged |
| `MobileRemoteServer.swift` | + `computer` and `device` artifact kinds |
| AGENTS.md | rewrite the two MCP sections into one "Built-in MCP (unified)" section when this ships |

## Phases

1. **Server core**: unified dispatcher in `mcp_host.rs`, sessions-as-actions,
   `help` action, dynamic advertising, schema-size regression test. Legacy
   browser server untouched.
2. **Browser domain**: extract shared impl, wire as `browser` action tool,
   single-config injection for new launches, legacy arms become shims.
   Update `verify-browser.sh` to drive the unified surface.
3. **Computer domain**: execute `docs/feature/computer-mcp.md` Phases 1–5,
   targeting the domain-module shape (its Phase 0 TCC spike is independent
   and can run before/alongside 1–2).
4. **Device domain**: settle the Node decision; engine probe + action tool +
   Settings panel + artifact kind; experimental flag
   (`UNPEEL_DEV_DEVICE_MCP`).
5. **Cleanup**: retire legacy browser argv arms after a deprecation window;
   AGENTS.md + `docs/feature/sessions-mcp.md` rewrites; release notes.

## Testing

- `cargo test`: action routing per domain, schema-size ceiling, advertising
  matrix (domain flags × engine presence), help output, legacy-shim parity
  for browser.
- `verify-browser.sh` / `verify-computer.sh` / (new) `verify-device.sh`
  drive the unified `__mcp__` surface end to end.
- Provider smoke: launch claude/codex/kimi/cursor once each; confirm exactly
  one `unpeel` server appears, tools resolve, and a browser action + a
  sessions action round-trip.

## Risks

- **Action-enum ergonomics on weaker CLIs**: enum tools are proven (Anthropic
  computer use), but kimi/grok/pi may misuse params without per-action
  schemas — mitigated by terse-but-precise descriptions, `help`, and
  actionable error strings ("action 'click' requires 'target'; call
  action:'help'").
- **Migration surface**: the injection matrix touches every provider; ship
  behind the existing experimental flags and verify per provider before
  graduating.
- **Sessions rename churn**: agents mid-flight during the update keep old
  tools until restart — fine (running server processes are pinned), but docs
  and `docs/feature/sessions-mcp.md` must not describe both surfaces at once;
  rewrite at Phase 5.
- **Device/Node decision** is a product call, not an engineering one — Phase
  4 is blocked on it, nothing else is.
