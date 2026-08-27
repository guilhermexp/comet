# Browser MCP Deep Check

Date: 2026-07-01. Verified live against `agent-browser` **0.31.1** (latest on
npm) on this machine (darwin-arm64, system Chrome installed). This document
corrects [ideas/browser_mcp.md](../../ideas/browser_mcp.md) where its
assumptions do not match the shipping engine, and pins the recommended
implementation.

> **Security scope (2026-08-14): browser profile isolation is not session
> process isolation.** Browser MCP gives each session an Unpeel-managed browser
> profile rather than the user's normal browser profile. Its On/Ask/Off choices
> are cooperative tool controls: hosted commands still run as the user's OS
> account and can read same-user state or invoke locally installed tools outside
> the MCP wrapper. Do not present Browser Ask/Off, site rules, or engine socket
> naming as a sandbox against malicious shell code. Elevated future modes must
> use a Host broker and OS-enforced confinement if they need that guarantee.

## Verified Findings

These change the plan. Each was tested, not assumed.

### 1. `agent-browser mcp` does not exist

The idea doc's process tree ends in "Exec `agent-browser mcp`". There is no
such subcommand: `agent-browser mcp` prints `Unknown command: mcp`, the native
binary contains no JSON-RPC/MCP-protocol strings, and the README never mentions
MCP. This is deliberate — the package's design philosophy is CLI-first for
agents (skills + shell, not MCP), so waiting for an upstream MCP mode is not a
plan.

**Consequence: Unpeel must author the MCP server itself.** The engine is a
tool-execution backend, not an MCP server to proxy.

### 2. The native binary is a thin client; the default daemon needs Node.js

`bin/agent-browser-darwin-arm64` (~5.8 MB) is a client that talks to a
per-session daemon over a Unix socket (`~/.agent-browser/<session>.sock`;
override dir with `AGENT_BROWSER_SOCKET_DIR`). On first command it spawns the
daemon:

- **Default daemon = Node.js + Playwright** (`dist/daemon.js` + an 84 MB
  `node_modules` tree: `playwright-core`, `webdriverio`, `node-simctl`,
  Appium client bits). With Node absent from `PATH` it hard-fails:
  `Failed to start daemon: No such file or directory`.
- Chromium itself is a separate download (`agent-browser install`) or a system
  browser via `AGENT_BROWSER_EXECUTABLE_PATH`.

The idea doc's "call the native binary directly and avoid adding a Node
runtime dependency" is therefore wrong **for the default engine mode**.

### 3. The experimental `--native` Rust daemon works with zero dependencies

Tested with `env -i PATH=/usr/bin:/bin` (no Node anywhere) invoking the Rust
binary directly with `AGENT_BROWSER_NATIVE=1`:

- Spawned **system Google Chrome** with `--remote-debugging-port=0` (raw CDP,
  no Playwright, no Chromium download).
- `open`, `snapshot -i` (accessibility refs `@e1…`), `screenshot`, `get`,
  `close` all worked.
- `AGENT_BROWSER_ALLOWED_DOMAINS` **is enforced** by the native daemon
  (disallowed navigation refused with a clear error).

This is the bundling-friendly path: one small signed binary + the user's
existing Chrome. It is marked experimental upstream; expect gaps vs the
Playwright daemon (video recording, traces, and PDF are Playwright features —
treat them as full-engine-only until verified).

### 4. iOS Simulator support is Node-daemon-only

The iOS path (`-p ios`) rides `webdriverio` + `node-simctl` + Appium inside the
JS daemon, and additionally requires Xcode and an Appium install. It cannot
exist in a native-only v1. Ship it later as part of an optional "full engine"
mode, health-check gated.

### 5. The env contract fits Unpeel's launcher model exactly

Everything Unpeel needs to control is env- or argv-addressable, per session:

| Engine env | Unpeel use |
| --- | --- |
| `AGENT_BROWSER_SESSION` | `unpeel-<session-id>` — one isolated browser per Unpeel session |
| `AGENT_BROWSER_SOCKET_DIR` | `~/.unpeel/browser/sockets` — never collide with a user's own agent-browser |
| `AGENT_BROWSER_NATIVE` | `1` in v1 (native CDP daemon) |
| `AGENT_BROWSER_EXECUTABLE_PATH` | Settings: detected Chrome/Chromium or custom path |
| `AGENT_BROWSER_HEADED` | Settings: default **headed** (Unpeel is a visual product) |
| `AGENT_BROWSER_DOWNLOAD_PATH` | `~/.unpeel/app-sessions/<id>/artifacts/browser/downloads` |
| `AGENT_BROWSER_ALLOWED_DOMAINS` | Settings site rules (engine-enforced, verified) |
| `AGENT_BROWSER_ACTION_POLICY` / `CONFIRM_ACTIONS` | later: approval policies (JSON policy file) |
| `AGENT_BROWSER_PROFILE` / `SESSION_NAME` / `STATE` | later: persistent/named profiles (elevated risk) |

Also relevant: `connect <port|url>` / `--cdp` / `--auto-connect` exist in the
CLI — the "Developer Mode: connect to live Chrome" feature maps directly onto
them with no extra engine work.

### 6. Miscellaneous verified facts

- License: Apache-2.0 (notice required in bundle). Binaries: darwin-arm64
  5.8 MB, darwin-x64 6.4 MB.
- The npm package ships `skills/agent-browser/SKILL.md` — a good, maintained
  source to adapt for the instructions/context payload (snapshot-then-act
  workflow, refs over selectors, re-snapshot after navigation).
- The daemon and its Chrome **outlive** both the CLI client and the MCP server
  process. Explicit lifecycle cleanup is required (below).
- Do **not** talk to the daemon socket protocol directly; it is internal and
  unversioned. Always go through the CLI client binary.

## Recommended Architecture

### Process tree (corrected)

```text
Unpeel session
  -> provider CLI with MCP config (claude/codex/cursor, same injection as Sessions MCP)
    -> unpeel-host __browser_mcp__        (Unpeel-authored stdio MCP server, Rust)
      -> exec bundled agent-browser client (per tool call, argv+env owned by Unpeel)
        -> per-session native daemon ── system Chrome (CDP)
```

`unpeel-host __browser_mcp__` is a sibling of the existing `__mcp__` mode: a
hand-rolled stdio JSON-RPC server in `crates/unpeel-core/src/browser_mcp.rs`,
cloning the proven `mcp_host.rs` skeleton (`run_stdio` → `handle_message` →
`tool_definitions` / `run_tool`). No MCP SDK. Caller identity via
`UNPEEL_SESSION_ID` from the inherited env, exactly like Sessions MCP.

Because the MCP server constructs the engine argv itself, the agent can never
pass policy-overriding flags (`--allowed-domains`, `--profile`, …). Policy is
enforced in Rust before every exec, re-reading `app-state.json` per call — the
same live-gate pattern as `load_security` in `mcp_host.rs`. This is the
decisive argument for the MCP route over putting the CLI on `PATH`: a
`PATH`-wrapper has no per-call choke point and flags can defeat env policy.
(A raw-CLI escape hatch can become a Developer Mode option later.)

### Engine modes

- **The only mode — Native**: bundled Rust binary, `AGENT_BROWSER_NATIVE=1`,
  system Chrome/Chromium (detected or custom path). Zero extra dependencies —
  critical because Unpeel's audience includes non-coders without Node.
- **Full engine (Node) — ruled out** (product decision 2026-07-02): Unpeel is
  a lightweight Swift app and will not ship or depend on a Node runtime +
  Playwright tree to unlock engine features. Consequence: video recording,
  traces, viewport streaming, and iOS Simulator Mobile Safari are unavailable
  until the **native Rust daemon** grows them — the realistic path is
  upstream contributions (CDP `Page.startScreencast` covers streaming and,
  frame-stitched, video; the daemon already speaks CDP). Anything gated on
  the Node daemon should be treated as "needs upstream", never "enable Node".

### Tool surface (v1)

Thin 1:1 translation onto CLI subcommands; return engine stdout (it is already
agent-optimized). Names are Unpeel-owned so the engine can change underneath.

```text
browser_open        open <url> [+ wait --load]
browser_snapshot    snapshot -i [-c]
browser_click       click @ref | selector
browser_fill        fill <target> <text>
browser_type        type/press/keyboard …
browser_get         get text|html|value|url|title [target]
browser_screenshot  screenshot <artifact-path> [--full|--annotate]
browser_wait        wait <selector|ms|--load state>
browser_scroll      scroll / scrollintoview
browser_console     console [--clear] (+ errors)
browser_close       close (this session's browser)
browser_context     read-only: access state, mode, engine version, artifact
                    dir, site rules, safety instructions (the belt-and-
                    suspenders instruction path from the idea doc)
```

Defer: tabs, network routing, storage/cookies, auth vault, diff, pdf, video,
mouse — add on demand. Keep `snapshot → act by @ref → re-snapshot` as the
documented core loop (adapt text from the package's own SKILL.md).

### Access grants

Mirror the Sessions MCP model in `state.rs` / `app-state.json`:

- `browser_access: { session-id → grant }` (deviations-only map) +
  `browser_default_access`.
- v1 levels: **Off** / **This session**. Default **Off** (roadmap rule: no
  risky default-on; the browser can see logged-in sites).
- Enforced per call in `browser_mcp.rs` (unknown caller → Off). Live for
  revoke; no restart needed to take access away.
- Launch injection rides the existing `mcp_enabled` boolean: inject both core
  servers whenever any core-MCP grant wants a client, gate each server per
  call. The manifest's `mcp_client_registered` + the existing
  restart-recommendation derivation extend to "browser access granted but
  session launched without the client" (new token `browser-access:on`) — same
  banner, no second banner path.

### Provider injection (all three reuse existing mechanisms)

- **Claude**: add an `unpeel-browser` entry beside `unpeel-sessions` in
  `~/.unpeel/mcp/claude-mcp.json` (`hook_assets.rs::write_claude_mcp_config`);
  the already-appended `--mcp-config` picks it up.
- **Codex**: wrapper adds `-c mcp_servers.unpeel-browser.*` overrides next to
  the existing `unpeel-sessions` block (env-passed session id, same reason).
- **Cursor**: second server entry in `~/.cursor/mcp.json`
  (`write_cursor_mcp_config`).

### Artifacts

```text
~/.unpeel/app-sessions/<session-id>/artifacts/browser/
  screenshots/   (server rewrites screenshot output paths here)
  downloads/     (AGENT_BROWSER_DOWNLOAD_PATH)
  logs/          (engine stderr, health reports)
```

Tool results return the stable absolute path. Videos/traces join when the full
engine mode ships. Retention: manual clear in Settings for v1.

### Lifecycle cleanup (new integration point)

The engine daemon + Chrome persist after the MCP server dies — by design. So:

- `UnpeelStore` kill/cleanup (`close_session`, prune) must also run
  `agent-browser close` for `unpeel-<session-id>` (or kill via the pid file in
  the socket dir).
- `rescan()` should GC orphaned daemons: socket-dir entries whose Unpeel
  session no longer exists.
- Restart keeps the same browser session key, so a restarted agent can reuse
  its still-warm browser.

### Health check (v1 scope)

bundled binary exists + executes + matches pinned version; Chrome/Chromium
found (or custom path valid); artifact dir writable; native daemon
starts/answers (`open about:blank` → `close` smoke); Node presence reported
(informational, unlocks full mode later); Xcode/Appium checks deferred to the
iOS pass. Report is copyable text, exposed in Settings and via
`browser_context`.

### Bundling and release

- Vendor the pinned binaries into `Unpeel.app/Contents/Resources/` (decide
  arm64-only vs +x64 with the app's Intel policy; +6.4 MB if both).
- Mark executable, sign with the app pipeline, verify notarization covers
  them, add the release-pipeline existence check (idea doc checklist stands).
- Include the Apache-2.0 notice.
- No npm anything at runtime or install time. Pin 0.31.1; re-verify the
  `--native` feature surface on every bump (it is still marked experimental).

## Suggested Slices

1. **Server skeleton**: `browser_mcp.rs` (`__browser_mcp__` argv mode), binary
   resolution, exec plumbing, `browser_context`, 5 core tools
   (open/snapshot/click/fill/screenshot). Testable standalone:
   `printf … | unpeel-host __browser_mcp__`.
2. **Grants + injection**: state fields, per-call gate, the three provider
   config writers, Settings page (enable card, status, per-session access),
   restart-recommendation token.
3. **Artifacts + lifecycle**: artifact dirs, path rewriting, close/prune/GC
   hooks in `UnpeelStore` + host.
4. **Health + diagnostics**: health check, trace-log lines
   (`browser-mcp` prefix in `~/.unpeel/hooks/trace.log`), remaining v1 tools.

Later: live Chrome CDP Developer Mode (`connect`/`--auto-connect`),
action-policy approvals/confirmations, upstream native-daemon
contributions for streaming/video (Node full-engine mode is ruled out — see
Engine modes above; iOS Simulator dies with it unless the native daemon's
WebDriver path matures).

## Open Questions — resolved vs remaining

- **Default access**: resolved — **On by default** (product decision
  2026-07-01, revising this doc's earlier Off recommendation): v1's engine is
  an isolated per-session profile with no access to the user's own
  logins/cookies, and Unpeel agents already run with full shell, so the
  sandboxed browser adds visibility rather than new privilege. Settings ▸
  Browser ▸ default Off is the master disable; per-session Off overrides block
  individual sessions. The roadmap's "no risky default-on" rule continues to
  apply to the elevated modes (shared/copied profile, live CDP), which must
  stay opt-in per session.
- **Mobile toggle**: superseded — iOS Simulator rides the Node daemon
  (Appium/WebdriverIO), which is ruled out; off the table unless the native
  daemon's WebDriver/Safari path matures upstream.
- **Appium managed vs diagnosed**: moot for the same reason.
- **Intel**: follow the app's overall Intel support decision; cost is one
  6.4 MB binary.
- **Artifact retention**: manual clear in v1; revisit age/size pruning with
  video support.
- **License-gated**: not separately gated in v1.
- **New/remaining**: how far the `--native` daemon's feature surface extends
  (pdf? cookies? tabs? network log?) — audit per tool before exposing each;
  whether headed-by-default (visual product) vs headless-by-default (less
  intrusive) is the right call — needs a product decision at Settings time.

## Native-Mode Findings After Shipping (2026-07-01, engine 0.31.1)

Verified while building the Settings/options pass:

- **Verified working in native mode**: `AGENT_BROWSER_PROFILE` (persistent
  profile, Chrome user-data-dir populated), `AGENT_BROWSER_COLOR_SCHEME`
  (`prefers-color-scheme` honored), headless default when `HEADED` unset,
  `ALLOWED_DOMAINS` enforcement (navigation refused off-list).
- **Engine bug — `PROFILE` + `ALLOWED_DOMAINS` together wedge the native
  daemon** at launch (~2.5 min then "daemon may be busy or unresponsive";
  reproduced 3×, each setting alone is fast). Mitigation shipped in
  `browser_mcp.rs`: site rules win, the profile is skipped while rules are
  set, and `browser_context` reports the downgrade. Re-test on engine bumps.
- **The native daemon hardcodes `--use-mock-keychain`** (2026-07-02): Chrome
  encrypts cookies with an ephemeral key per launch, so encrypted cookies
  NEVER survive a browser restart in a kept profile — Chrome purges
  undecryptable cookies at startup (verified: a plaintext cookie survives, all
  encrypted ones vanish; the engine Chrome can't even keep its own logins
  across restarts). Consequences: (a) "seed from Chrome" cookie-DB copying is
  fundamentally impossible against this engine and was removed after
  shipping briefly; (b) kept-per-project **logins** work via the engine's own
  state save/restore instead — `AGENT_BROWSER_SESSION_NAME=unpeel-proj-<root>`
  reads cookies over CDP at runtime and re-injects on launch (verified,
  including sharing across engine sessions with the same name; state lands in
  `~/.agent-browser/sessions/`). Future "import my Chrome logins" needs
  user-consented Safe-Storage decryption + `cookies set` injection, not file
  copying. Also note: state JSONs store cookies in plaintext unless
  `AGENT_BROWSER_ENCRYPTION_KEY` is set — worth wiring before release.
- **`AGENT_BROWSER_STREAM_PORT` is a no-op in native mode** — nothing listens
  on the port; viewport WebSocket streaming (pair browsing) lives in the
  Node/Playwright daemon only. Consequence: browser streaming to remote
  clients (iOS / connected Macs via `unpeel-host __remote__`) requires either
  the full-engine (Node) mode or an upstream contribution adding CDP
  `Page.startScreencast` to the Rust daemon (it already speaks CDP, so this
  is the natural PR). Interim options that work with native mode today:
  screenshot-poll preview (~1 fps via the existing exec plumbing) and
  serving `artifacts/browser/` over the remote server.
