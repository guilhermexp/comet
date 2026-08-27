# Computer Use MCP — design & implementation plan

> **Security containment (2026-08-14): development builds only.** The current
> embedded daemon inherits Unpeel.app's Accessibility and Screen Recording TCC
> grants, runs unrestricted, and listens on a same-user Unix socket. A hosted
> command could call that raw socket without going through the Ask/Off and app
> allowlist checks in `computer_mcp.rs`. Production builds therefore hide the
> feature, force its launch flag off, stop/remove stale daemon state, and omit
> `cua-driver` from the app bundle. The approval UI remains useful for
> cooperative development agents, but is explicitly not isolation from code
> running as the user's account. Shipping Computer Use again requires a
> Host-owned privileged broker and an OS-enforced boundary that prevents hosted
> code from reaching the engine directly.

> **⚠️ Engine replaced (2026-07-22, pre-release): Peekaboo → cua-driver.**
> The `computer` domain now runs on **cua-driver** (github.com/trycua/cua,
> `libs/cua-driver`, MIT, pure Rust; pinned target 0.10.0). The feature was
> unreleased, so the action surface was reshaped to cua-driver's model at
> the same time — nothing below about Peekaboo argv/tools governs anymore;
> the cooperative access model (`Ask` default + `/mcp/approve-computer` approval flow,
> `computer_approvals` prune/carry, live per-call gate), artifacts contract,
> and phone-gallery kind are unchanged. What changed and why:
>
> - **Background control.** cua-driver drives apps through AX actions and
>   per-pid event posting: no focus steal, the user's real cursor never
>   moves (a per-session overlay cursor glides instead). This is the
>   capability that actually matches Unpeel's multi-session product shape —
>   Peekaboo's real-CGEvent model made one computer session hijack the
>   physical mouse.
> - **Structural TCC attribution.** The native app spawns the engine daemon
>   (`cua-driver serve --embedded --socket ~/.unpeel/computer/daemon.sock`)
>   as a **direct child** (`ComputerEngineManager.swift`) per the embedding
>   contract (`rust/Skills/cua-driver/EMBEDDING.md` in the cua repo): grants
>   attribute to Unpeel.app, one Settings entry, no second prompt. The app
>   probes/requests grants natively (`AXIsProcessTrusted`,
>   `CGPreflightScreenCaptureAccess` + request APIs, ComputerPermissions.swift)
>   and restarts the daemon when grants change (per-process TCC cache). The
>   daemon runs `CUA_DRIVER_PERMISSION_MODE=unrestricted` (documented
>   two-env contract) because Unpeel owns the cooperative approval UX. Never spawn the
>   daemon from a session host or via `open`/NSWorkspace.
> - **One-shot calls against the daemon socket.** `computer_mcp.rs` invokes
>   `cua-driver call <tool> '<json>' --socket …` per action (same bounded
>   exec pattern as before; `call` fails cleanly when the daemon is down and
>   the error tells the agent to have the user open Unpeel). The server
>   builds each tool-arg JSON from a per-action whitelist — agents can't
>   smuggle `session`/debug/output fields.
> - **Window-first action surface** (18 actions): `apps`, `launch`, `quit`,
>   `front`, `windows`, `see`, `screenshot`, `desktop`, `click`, `type`,
>   `set_value`, `press`, `hotkey`, `scroll`, `drag`, `move_cursor`,
>   `escalate`, `context`. The loop is launch → pid + windows → `see`
>   (accessibility tree with `[N]` element indices PLUS a screenshot
>   artifact from one call) → act by `element_index` → re-`see` to verify.
>   Screenshots ride `--screenshot-out-file` into
>   `artifacts/computer/screenshots/` (stdout stays base64-free; an inline
>   `screenshot_png_b64` is stripped defensively).
> - **Sessions & the scope ladder.** Each Unpeel session maps to cua session
>   `unpeel-<id>` (auto-declared, `capture_scope: auto`): its own overlay
>   cursor, and desktop-scope perception/input is locked until the agent
>   explicitly calls `escalate` after exhausting the window ladder — a real
>   safety upgrade over the old whole-screen-by-default model.
>   `__computer_cleanup__ <id>` ends the cua session on remove/archive/stop
>   (wired next to `__browser_cleanup__`).
> - **macOS 15 floor dropped.** cua-driver captures via the `screencapture`
>   CLI; the spawn gate no longer checks the OS version.
>
> **Validated headless (2026-07-22):** cua-driver 0.10.0 built from source
> and installed at `~/.unpeel/computer/bin/cua-driver`; smoke-tested the
> exact contract computer_mcp.rs uses — embedded `serve --socket` +
> unrestricted two-env launch, `call <tool> '<json>' --socket` one-shots,
> `check_permissions` shape (`attribution: "host"`), `start_session`
> idempotence, `launch_app`/`list_windows`/`get_window_state` shapes
> including the `degraded`/`escalation` contract, `end_session`, and the
> "daemon is not running" error string the Rust side rewrites. One caveat
> confirmed live: window capture can fail ("could not create image from
> window") even while `check_permissions` reports `screen_recording: true`
> when the responsible identity doesn't truly hold the grant — exactly the
> per-identity behavior EMBEDDING.md documents. Socket paths must stay
> under the 104-byte SUN_LEN limit (`~/.unpeel/computer/daemon.sock` is
> fine; beware very deep custom `UNPEEL_HOME`s).
>
> **Still open:** end-to-end validation with the daemon as Unpeel.app's
> child on a granted machine (screenshots + background input under the
> app's own TCC identity), vendoring a pinned release binary for
> `build-app.sh` (the bundling + re-sign step is written and picks up
> `~/.unpeel/computer/bin/cua-driver`), `verify-computer.sh`, and a
> decision on surfacing the engine's browser/recording tool families
> (deliberately not exposed — Unpeel has its own browser domain, and
> recording is a product question).
>
> **Historical note (2026-07-18):** the tool surface lands as the `computer`
> domain of the **unified MCP server** — one action-enum tool, never a
> fourth server process (`docs/feature/unified-mcp.md`). The Peekaboo-based
> design below is kept for the record — cooperative access model, artifacts, gallery, and
> native-UI sections still describe the shipped shape.

Status: **plan, not built** (2026-07-17). This doc is the design for a third
first-party MCP server that gives an agent session control of the Mac itself —
screenshots, clicking, typing, app/window/menu control — mirroring the Browser
MCP architecture (`crates/unpeel-core/src/browser_mcp.rs`,
`docs/feature/browser-mcp-deep-check.md`).

## Summary

- New server: `crates/unpeel-core/src/computer_mcp.rs`, run as
  `unpeel-host __computer_mcp__` (stdio JSON-RPC, hand-rolled like
  `mcp_host.rs`/`browser_mcp.rs`, no SDK). Caller identity via
  `UNPEEL_SESSION_ID`.
- Engine: **Peekaboo** (github.com/openclaw/Peekaboo) — a native Swift CLI
  (MIT, ~98% Swift, no Node runtime) built on ScreenCaptureKit + the
  Accessibility APIs. Bundled next to `unpeel-host` in the app bundle, exactly
  like `agent-browser`. The server builds every argv itself, so agents can
  never pass policy-overriding flags.
- Screenshots and `see` captures land in
  `~/.unpeel/app-sessions/<id>/artifacts/computer/screenshots/` and tools
  return the **file path**, never inline image bytes — same contract as the
  Browser MCP, so the phone gallery/thumbnail pipeline picks them up with a
  one-line artifact-kind addition.
- Cooperative access policy **inverts** the browser default: computer use sees the user's
  real screen and drives the real mouse/keyboard, so the app-wide default is
  **Ask** (per-session approval via the existing `/mcp/approve-*` alert
  pattern), never On-by-default. This policy does not isolate malicious
  same-user shell code; production containment is described at the top.

## Product fit

Computer use is domain-agnostic — it serves research, ops, QA, personal
automation, not just coding — so it fits the "for everything" thesis. It also
feeds the North Star review surface directly: every action can produce a
screenshot artifact that shows up in the desktop and phone galleries
("screenshots are the review surface", AGENTS.md). Nothing here adds IDE
chrome; the agent's view of the Mac is images + text, rendered through
existing surfaces.

## Engine: Peekaboo

Verified facts (v3.9.3, 2026-07-17):

- Native Swift binary, macOS **15.0+ (Sequoia)**, MIT license. The npm MCP
  wrapper is Node-based but **optional** — we skip it entirely (Unpeel never
  ships or requires Node; we author our own MCP server anyway, same rationale
  as the browser engine).
- CLI surface: `see` (capture + annotated UI element map → snapshot id +
  opaque element ids, `--json`), `image`, `click --on <id> --snapshot <id>`,
  `type`, `press`, `hotkey`, `scroll`, `drag`, `window
  list|move|resize|focus`, `app launch|quit|switch|list`, `menu
  list|click`, `dialog list|click|input|dismiss`, `space`,
  `permissions status|request-*`, `config`, plus an `agent` command (its own
  LLM loop) and a `run` script runner — both **excluded** (the Unpeel session
  *is* the agent; we never wire Peekaboo's AI provider config).
- Process model: v3 uses a **warm daemon + "Peekaboo.app bridge"**
  (lease-owned sockets, local fallback) for background input delivery. This
  is the biggest unknown — see Phase 0 spike and Risks.
- `see --json` is the interaction model: it returns a snapshot id and element
  ids that subsequent `click/scroll/set-value` calls target via
  `--snapshot`/`--on`. This maps 1:1 onto the Browser MCP's
  `browser_snapshot` → `@e1` ref flow, so the tool ergonomics agents already
  know transfer directly.

Why wrap the CLI instead of using Peekaboo's built-in `mcp serve`: Unpeel owns
the tool schema (stable across engine bumps), the access gate (re-read from
`app-state.json` per call, applies live), artifact routing into the session
dir, and argv construction (no policy-overriding flags reachable). Same
reasons the Browser MCP was authored in-house even though its engine now has
alternatives.

## ⚠️ macOS version floor

The native app targets **macOS 13** (`apps/native/UnpeelNative/Package.swift`
line 16: `.macOS(.v13)`; `build-app.sh` bundles with
`--minimum-deployment-target 13.0`). Peekaboo requires **macOS 15**. We do
NOT raise the app floor. Instead:

- `spawnSession` gates the launch flag: `computer_mcp_enabled` is forced
  `false` when `ProcessInfo.operatingSystemVersion.majorVersion < 15`, so
  pre-Sequoia Macs never inject the client.
- The Settings panel's engine probe reports "Requires macOS 15" as a distinct
  status (alongside "Not found").
- The server itself also preflights the OS per call and returns a clear
  error, as belt-and-braces for stale injections.

## Architecture

Mirror set of the Browser MCP, symbol for symbol:

| Browser MCP | Computer MCP |
| --- | --- |
| `browser_mcp.rs` | `computer_mcp.rs` (new) |
| `BROWSER_MCP_ARG = "__browser_mcp__"` | `COMPUTER_MCP_ARG = "__computer_mcp__"` |
| `BROWSER_CLEANUP_ARG = "__browser_cleanup__"` | `COMPUTER_CLEANUP_ARG = "__computer_cleanup__"` (see Lifecycle) |
| `unpeel-browser` (server name) | `unpeel-computer` |
| `resolve_engine_binary()` | same order: `UNPEEL_PEEKABOO_BIN` env → sibling of `unpeel-host` (packaged) → `~/.unpeel/computer/bin/peekaboo` → PATH (dev) |
| `load_security()` / `load_options()` per call | same pattern, reading the new state fields per call (live gate) |
| `BrowserAccess` / `browser_default_access` | `ComputerAccess` / `computer_default_access` |
| `SessionHostLaunch.browser_mcp_enabled` | `SessionHostLaunch.computer_mcp_enabled` |
| manifest `browser_client_registered` | manifest `computer_client_registered` |

`crates/unpeel-host/src/main.rs` gains two dispatch arms next to the
`__browser_mcp__`/`__browser_cleanup__` arms (lines 39–54 today).

Engine execution follows `exec_engine_with` (`browser_mcp.rs:504`): spawn the
bundled `peekaboo` with a bounded timeout, capture stdout/stderr, truncate
output (`truncate_output`), trace to `~/.unpeel/hooks/trace.log` with a
`computer-mcp` prefix.

### No per-session isolation — by nature, not omission

The browser is isolated per session (own profile/window). The computer is
**one shared screen, one pointer, one keyboard focus**. Consequences:

- Snapshot ids from `see` are keyed per caller session in server memory, but
  two sessions driving the screen concurrently will interleave. The server
  does not attempt locking in v1; the tool descriptions state plainly that
  actions are visible to and contended with the user and other sessions.
- This asymmetry is the core reason the access default is `Ask`, below.

## Cooperative access model

`ComputerAccess` (state.rs, next to `BrowserAccess`):

```rust
#[serde(rename_all = "lowercase")]
pub enum ComputerAccess { Off, #[default] Ask, Allow }
```

- **App-wide default: `Ask`** (contrast `BrowserAccess` which defaults `On` —
  justified there by per-session profile isolation; here the agent reads the
  user's real screen, which can contain password managers, mail, other
  sessions' secrets, and drives real input).
- `from_state_str` parses fail-closed: unknown/missing → `Off` in the reader
  used by the gate? **No** — missing field must yield the serde default
  `Ask`; only an *unrecognized* explicit value falls back to `Off`. (Mirror
  the deliberate asymmetry documented at `BrowserAccess::from_state_str`,
  state.rs:323, but with `Ask` as the absent-field default.)
- Persisted as `computer_default_access` on `AppState`
  (`#[serde(default)]`), written by
  `UnpeelStore.setDefaultComputerAccess` → `mutateAppStateJSON`, re-read by
  the server **per tool call** so Settings changes apply live (`Off` revokes
  running sessions immediately; `Allow`/`Ask` transitions need no restart).

### Per-session approval under `Ask`

Reuse the MCP write-approval machinery wholesale
(`MCPWriteApproval.swift`, `HookServer.swift`, `mcp_host.rs::
request_write_approval` — fact-checked shapes below):

- New route `POST /mcp/approve-computer` on the hook server. Request:
  `{"session_id": "<caller>"}`. Response: `{"approved": true|false}`.
  Covered automatically by the shared `x-unpeel-auth` header
  (`mcp_auth.rs`) like every `/mcp/*` route.
- Server side: on the first gated tool call from a session whose id is not in
  the remembered set, `computer_mcp.rs` POSTs the route with a **130s**
  client timeout; the HookServer per-request ceiling for this path is raised
  to **150s** (same numbers as `/mcp/approve-write`, HookServer.swift:270).
- Native side: coalescing NSAlert queue ("Session *X* wants to control this
  Mac — it will be able to see the screen and use the mouse and keyboard.
  Allow / Deny"), FIFO, identical requests coalesce.
- Approved ids persist in `computer_approvals: Vec<String>` on `AppState`
  (session ids, not pairs — the "target" is always the Mac). Lifecycle
  matches `mcp_write_approvals`: `pruneNativeState` drops removed sessions'
  ids; `restartSession` snapshots before the prune and re-adds under the new
  session id (read-before-prune discipline). Deny fails the tool call with a
  clear "the user declined; do not retry" message.
- A `computer_context` tool (mirror of `browser_context`) is **always**
  callable and explains the current access state without triggering a prompt.

### App allowlist (analog of `allowed_domains`)

`ComputerSettings.allowed_apps: String` (comma/space list of app names or
bundle ids; empty = all). Enforcement is server-side and best-effort: action
tools that carry an app target (`computer_app`, `computer_window`, `see
--app`, menu/dialog tools) are checked before exec; global-coordinate clicks
are checked against the frontmost app (via `peekaboo app list --json`) at
call time. Documented as a guardrail, not a sandbox — the honest framing the
browser doc uses for its own rules.

## Tool surface (v1: 12 tools)

Names/args are Unpeel-owned; each maps to one engine invocation.

| Tool | Engine argv (server-built) | Notes |
| --- | --- | --- |
| `computer_see` | `peekaboo see [--app <app>] --json` + annotated capture into artifacts | The primary loop starter: returns element map text (ids, roles, labels) **plus** the saved screenshot path. Snapshot id cached per caller session. |
| `computer_click` | `peekaboo click --on <id> --snapshot <id>` or `--coords x,y` | Element id from the last `see`; re-`see` after UI changes (refs go stale — same guidance as browser refs). |
| `computer_type` | `peekaboo type --text <s>` | Optional `clear`. |
| `computer_press` | `peekaboo press <key> [--count n]` | Named keys (return, escape, tab, arrows…). |
| `computer_hotkey` | `peekaboo hotkey <mods,key>` | e.g. `cmd,shift,t`. |
| `computer_scroll` | `peekaboo scroll --on <id> --direction d --amount n` | |
| `computer_screenshot` | `peekaboo image --mode screen\|window --path <artifact>` | Returns the artifact path. `--retina` per settings. |
| `computer_app` | `peekaboo app launch\|quit\|switch\|list` | Allowlist-checked. |
| `computer_window` | `peekaboo window list\|focus\|move\|resize` | Allowlist-checked. |
| `computer_menu` | `peekaboo menu list\|click` | Allowlist-checked. |
| `computer_dialog` | `peekaboo dialog list\|click\|input\|dismiss` | For system dialogs/sheets. |
| `computer_context` | (no engine call) | Access state, permission status (`peekaboo permissions status`), allowlist, engine path/version. Always callable. |

Excluded from v1: `drag`/`swipe`/`move` (add later if agents need them),
`space`, `dock`, `set-value`/`perform-action` (fold into click/see if useful),
and permanently: `agent`, `run`, `config` (policy surface, never
agent-reachable).

Server-side schema tests mirror `browser_mcp.rs` tests
(`tool_definitions_have_valid_schemas`, required-arg validation, etc.).

## Artifacts & gallery (hard requirement: same as Browser MCP)

- Screenshots and `see` captures write to
  `<session>/artifacts/computer/screenshots/` (timestamped filenames, same
  convention as `browser_artifacts_dir`, browser_mcp.rs:490). Tools return
  `Saved screenshot to <path>` — never base64.
- Phone/desktop gallery: in `MobileRemoteServer.swift`, add `"computer"` to
  `listedArtifactKinds` (line 1549) and a case in `artifactKindDir`
  (lines 1558–1570) mapping `"computer"` → `artifacts/computer/screenshots`.
  Listing, byte-slicing, `max_dim` thumbnails (cached under
  `artifacts/thumbs/`), and delete then work unchanged. Older phone builds
  simply don't request the new kind — additive, no protocol bump.

## State additions (`state.rs`)

```rust
#[serde(default)] pub computer_default_access: ComputerAccess,   // Ask
#[serde(default)] pub computer_approvals: Vec<String>,           // session ids approved under Ask
#[serde(default)] pub computer_settings: ComputerSettings,
```

`ComputerSettings` v1 (all `#[serde(default)]`, read per engine call like
`load_options`):

| field | type | default | meaning |
| --- | --- | --- | --- |
| `allowed_apps` | `String` | `""` | app/bundle-id allowlist; empty = all |
| `retina` | `bool` | `false` | capture at 2x (bigger artifacts, sharper review) |

Deliberately minimal — no profile modes, no headed toggle (there is only the
real screen).

## Launch injection (per provider — mirror of the browser wiring)

- `SessionHostLaunch.computer_mcp_enabled: bool` (`#[serde(default)]`,
  false), set by `spawnSession` from
  `computer_default_access != .off` **and** feature flag on **and**
  macOS ≥ 15. Recorded in the manifest as `computer_client_registered`
  (same four manifest-creation sites as `browser_client_registered`,
  session_host.rs:2051/2159/2338/2356).
- **Claude**: `install_claude_hooks` additionally calls a new
  `write_claude_computer_mcp_config()` →
  `~/.unpeel/mcp/claude-computer-mcp.json` (`unpeel-computer`, args
  `["__computer_mcp__"]`, exe path refreshed every launch);
  `claude::startup_command` grows a third additive `--mcp-config` gated on
  the flag (existing skip-if-user-passed-`--mcp-config` behavior unchanged).
- **Codex**: `codex::configure_host_command` exports
  `UNPEEL_COMPUTER_MCP_BIN` when enabled; the wrapper script
  (`CODEX_WRAPPER_SCRIPT`, hook_assets.rs:~1256) grows a third guarded block
  injecting `-c mcp_servers.unpeel-computer.command/args/env`
  (with explicit `UNPEEL_SESSION_ID`, since Codex spawns MCP servers with a
  minimal env).
- **Kimi**: `write_kimi_computer_mcp_config()` via the generic
  `write_kimi_mcp_file(path, "unpeel-computer", COMPUTER_MCP_ARG)`
  (hook_assets.rs:2020); `kimi::startup_command` appends another repeatable
  `--mcp-config-file`, preserving the implicit `~/.kimi/mcp.json` append.
- **Cursor**: `write_cursor_mcp_config` (hook_assets.rs:1584) takes the new
  flag and merges/prunes an `unpeel-computer` entry (called from `run_host`,
  session_host.rs:2080).
- Flip semantics match browser: turning access **on** reaches existing
  sessions at their next natural restart (no restart banner); **off** applies
  live through the per-call gate.

## Native UI

- **Feature flag**: `ExperimentalFeature.computerUse` (env escape hatch
  `UNPEEL_DEV_COMPUTER_USE`, default off), added to `ExperimentalFeature.all`
  (FeatureFlags.swift:79). Gates the Settings tab and the spawn-time
  injection flag. Ship dark → experimental → default-visible, same ramp as
  Sessions MCP.
- **Settings ▸ Computer** (`ComputerSettingsPanel`, modeled on
  `BrowserSettingsPanel`, SettingsView.swift:1094):
  - Engine status probe (async `.task`): resolves `peekaboo` in the same
    order as the server; states: Checking / Not found / Requires macOS 15 /
    path shown.
  - **Permissions probe**: runs `peekaboo permissions status` and renders
    Screen Recording / Accessibility / Event Synthesizing rows with
    grant buttons (deep-link to System Settings panes; Peekaboo's
    `request-*` subcommands can drive the prompts).
  - Access picker (Off / Ask each session / Allow) →
    `store.setDefaultComputerAccess`.
  - "Approved sessions" list with per-row Revoke (mirrors the MCP
    write-approval list) → edits `computer_approvals`.
  - Allowed apps field + retina toggle → `store.updateComputerSettings`.
- Store: `@Published computerDefaultAccess/computerSettings`, write paths
  through `mutateAppStateJSON` (field-preserving read-modify-write, the same
  funnel as `persistBrowserDefaultAccess`, UnpeelStore.swift:1594).

## Missing-permission help (shipped 2026-07-18)

When required TCC grants are missing, the failure is made actionable at every
layer instead of dead-ending on a raw engine error:

- `context` parses `peekaboo permissions` into per-row granted/NOT granted
  lines and, when required grants are missing, tells the agent plainly what
  the user must do (System Settings ▸ Privacy & Security; Settings ▸ Computer
  grant buttons) and that a retry works without an app restart.
- Every failed engine action (`exec_engine`) re-probes permissions; if
  required grants are missing it rewrites the error into the same guidance
  **and** fires `POST /mcp/computer-permissions-needed` at the app —
  fire-and-forget, 5s timeout, deduplicated app-side per missing-set per app
  run — which shows an alert with "Open Screen Recording"/"Open
  Accessibility" buttons (`ComputerPermissions.swift`).
- The Ask-mode approval alert chains into the same check on Allow: the user
  just engaged, so if grants are missing they get the grant prompt
  immediately instead of a failing first action.
- Settings ▸ Computer re-probes on `NSApplication.didBecomeActiveNotification`
  so a grant made in System Settings shows as soon as the user switches back.

## TCC permissions (the real integration risk)

Peekaboo needs **Screen Recording**, **Accessibility**, and (for background
input) **Event Synthesizing**. macOS attributes these to the *responsible
process* — for a binary bundled and signed inside Unpeel.app, that should be
Unpeel itself, meaning one grant covers the feature and survives updates
(stable Developer ID). But the chain here is deep (app → session host →
MCP server → peekaboo → its warm daemon → possibly a "Peekaboo.app" bridge),
and TCC attribution through spawned helpers is exactly where surprises live.
Dev builds sign with a different (local Apple Development) identity than
release, so grants are per-flavor.

**This is Phase 0 and must be proven before any other phase starts:**

1. Bundle a pinned `peekaboo` into a dev `dist/Unpeel.app`, re-signed.
2. From a real hosted session (spawned through `unpeel-host`), run
   `peekaboo permissions status`, then `see`, `click`, `type` against a test
   app. Record which TCC identity the grants attach to (System Settings ▸
   Privacy & Security), and whether the warm daemon / Peekaboo.app bridge is
   required or whether pure-CLI fallback works. If the bridge app is
   mandatory, decide: bundle it as a helper (signing/notarization
   implications) or pin an engine version where CLI-only works.
3. Repeat on the release-signing path (`--dry-run` release build) to confirm
   grant stability across rebuilds.

## Lifecycle & cleanup

Peekaboo's warm daemon outlives individual CLI calls (like the browser
engine's daemon). Unlike the browser there is one daemon per user, not per
session, so cleanup is **not** per-session:

- `__computer_cleanup__` (no session arg) stops the daemon; wired into app
  quit and Settings ▸ Off, **not** into `killAndCleanup` per session.
- Exact daemon stop mechanism (socket file? `peekaboo` subcommand?) is a
  Phase 0 finding; if the daemon self-idles, this mode may be a no-op — keep
  the argv arm anyway so the contract exists.

## Bundling & signing (`build-app.sh`)

Mirror the agent-browser section (lines 125–159, 269–282):

- Resolve source: `UNPEEL_PEEKABOO_BIN` env → `~/.unpeel/computer/bin/peekaboo`
  → `command -v peekaboo` (brew, for dev). `cp -L` into
  `Contents/MacOS/peekaboo`; copy the MIT LICENSE into
  `Contents/Resources/peekaboo-LICENSE.txt`; add a `codesign_release` line so
  notarization covers it. Missing engine = non-fatal `note:` in dev; treat as
  a release blocker once the feature ships (same policy as agent-browser).
- **Pin an exact Peekaboo release** and record it here + in RELEASE.md. The
  repo lives at `openclaw/Peekaboo` (moved/forked from steipete's original;
  the brew tap is still `steipete/tap`) — vendor the binary from a pinned
  GitHub release, never a moving tap. Measure the binary size in Phase 0 and
  note the DMG delta.
- Every version bump runs `verify-computer.sh` (below) before the pin is
  raised — the same discipline as `verify-browser.sh` for agent-browser.

## Testing

- `cargo test --manifest-path crates/Cargo.toml` — new unit tests in
  `computer_mcp.rs`: tool schemas valid, required args enforced, allowlist
  parsing, access-gate refusal strings, artifact path construction.
- `apps/native/verify-computer.sh` (new, modeled on `verify-browser.sh`):
  1. **engine**: binary resolves; `permissions status` parses; `see --json`
     returns a snapshot id + element ids against a scratch app (TextEdit).
  2. **gate**: with access `off`, every tool except `computer_context`
     returns `"isError":true` mentioning computer access.
  3. **mcp**: end-to-end `__computer_mcp__` — `computer_see` +
     `computer_screenshot` produce files under
     `artifacts/computer/screenshots/` ("Saved screenshot to").
  4. **allowlist**: `allowed_apps` blocks an off-list `computer_app switch`.
  5. **approval**: under `Ask` with no app running, the tool call fails
     closed with the "approval unavailable" error (headless can't answer the
     alert — asserting fail-closed is the point).
- `swift build` / `swift test` in `apps/native/UnpeelNative` (panel, store
  writes, prune/carry of `computer_approvals`).
- Manual: grant flow on a clean `dev:native:blank` instance; gallery shows
  computer screenshots on the phone.

## Implementation phases

1. **Phase 0 — TCC/daemon spike** (blocking): bundling, permission
   attribution, daemon/bridge behavior, binary size, pin a version. Output:
   go/no-go + corrections to this doc.
2. **Phase 1 — Rust server**: `computer_mcp.rs` (12 tools, gate, allowlist,
   artifacts), state.rs fields, main.rs dispatch, unit tests.
3. **Phase 2 — launch plumbing**: `SessionHostLaunch.computer_mcp_enabled`,
   manifest flag, all four provider injections, macOS-15 spawn gate.
4. **Phase 3 — native**: feature flag, Settings ▸ Computer panel
   (probe/permissions/picker/approvals/options), store write paths,
   `/mcp/approve-computer` route + alert, approval prune/carry.
5. **Phase 4 — artifacts**: gallery kind, phone verification.
6. **Phase 5 — ship prep**: build-app.sh bundling, `verify-computer.sh`,
   AGENTS.md + provider docs updates, release-notes entry; graduate the flag
   when stable.

## Risks & open questions

- **TCC attribution through the spawn chain** — Phase 0 exists because of
  this; everything else is pattern-following.
- **Peekaboo.app bridge**: if background input delivery requires shipping a
  helper app, bundling/notarization cost goes up; CLI-foreground-only might
  be an acceptable v1 (actions require the target app frontmost).
- **Repo provenance / maintenance**: `openclaw/Peekaboo` post-fork cadence is
  unproven; pinning + verify script bounds the blast radius, and the MIT
  license permits a hard fork if upstream stalls.
- **macOS 15 floor** vs the app's macOS 13 target — handled by gating, but
  support burden ("why is Computer greyed out") lands on the Settings copy.
- **Multi-session contention** on one screen — documented, not solved, in v1.
- **Screen contents are secrets**: even read-only `see`/`screenshot` can
  capture password managers or other sessions. The `Ask` default, the
  approval alert copy, and the allowlist are the mitigations; the docs must
  not overclaim isolation (there is none — this is the anti-browser).
- Open: exact daemon shutdown mechanism; whether `see` annotation output
  needs size capping (`AGENT_BROWSER_MAX_OUTPUT` analog); whether
  `computer_dialog` should be allowlist-exempt (system dialogs belong to no
  app).
