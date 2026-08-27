# Unpeel Device MCP

Unpeel's third first-party MCP server: it gives an agent session control of
iOS/Android simulators, emulators, and physical devices through the
[callstack/agent-device](https://github.com/callstack/agent-device) engine
(MIT). Built 2026-07-03, modeled 1:1 on the Browser MCP
(`docs/feature/browser-mcp-deep-check.md`); this doc covers what differs.

## The Node carve-out

`agent-device` is Node/TypeScript to the core: `engines: node >= 22.19`, the
bin is a `.mjs`, and the per-platform native helpers (XCTest `apple-runner`,
Android snapshot helper, macOS helper) are driven by a Node daemon. There is
no native mode like agent-browser's pure-Rust `--native` CDP daemon, and none
is realistic upstream (the iOS driver fundamentally needs Xcode tooling).

The 2026-07-02 product rule — Unpeel never ships or requires a Node runtime —
is therefore **relaxed for this feature only** (decision 2026-07-03):

- Unpeel still never *ships* Node or the engine. `build-app.sh` does not
  bundle anything for the Device MCP.
- The feature is **bring-your-own-toolchain**: it already requires Xcode
  (iOS) or the Android SDK (Android), so `npm install -g agent-device` plus
  Node 22+ is the lightest of its prerequisites.
- A missing engine degrades gracefully: every gated tool returns install
  guidance, `device_context` reports engine + Node availability, and
  Settings ▸ Devices shows both probes.
- Do **not** extend this carve-out to other Node-only features without an
  explicit product decision.

## Architecture

```
provider CLI (claude / codex)
  └── unpeel-host __device_mcp__        crates/unpeel-core/src/device_mcp.rs
        └── agent-device CLI            user-installed, Node 22+
              └── shared daemon         AGENT_DEVICE_STATE_DIR=~/.unpeel/device/state
                    ├── XCTest runner   iOS/tvOS (needs Xcode)
                    └── adb + helper    Android (needs Android SDK)
```

- One engine CLI invocation per tool call, argv built server-side — agents
  can never pass policy-overriding flags (same rationale as the browser).
- Session isolation via env, not flags: `AGENT_DEVICE_SESSION=
  unpeel-<session-id>` pins the engine session;
  `AGENT_DEVICE_STATE_DIR=~/.unpeel/device/state` keeps Unpeel's daemon
  separate from the user's own agent-device use.
- **One shared daemon** serves all Unpeel sessions (unlike the browser's
  per-session daemons). Engine sessions are isolated within it. Caveat:
  simulator runner leases are daemon-wide, so a *second* daemon (e.g. the
  user's personal agent-device with its default state dir) can hold a
  simulator's runner and the engine reports "already owned by another
  agent-device daemon" — that error is accurate and self-explaining.
- `__device_cleanup__ <id>` (spawned by `UnpeelStore.killAndCleanup`) closes
  the engine *session*; the shared daemon deliberately stays for other
  sessions and idles when unused.

## Tool surface (14)

| Tool | Engine argv | Notes |
| --- | --- | --- |
| `device_context` | — | Always callable; access state, engine/Node probes, artifact dir |
| `device_devices` | `devices [--platform p]` | Simulators, emulators, connected devices |
| `device_apps` | `apps --platform p [--apps-filter all]` | Launchable apps |
| `device_open` | `open <app> [url] --platform p [--device d] [--relaunch]` | 300s timeout (cold boot + runner build) |
| `device_snapshot` | `snapshot [-i] [-s scope] [--force-full]` | Accessibility tree with `@e1` refs |
| `device_press` | `press <target>` / `longpress <target> <ms>` | Tap; `duration_ms` ⇒ long press |
| `device_fill` | `fill <target> <text>` | Replaces field content; refuses empty text |
| `device_type` | `type <text>` | Appends to the focused field |
| `device_scroll` | `scroll <dir> [--pixels n]` | up/down/left/right/top/bottom |
| `device_screenshot` | `screenshot <artifact-path> [--overlay-refs]` | Saves under `artifacts/device/screenshots/` |
| `device_wait` | `wait <ms>` / `wait text <t> [timeout]` / `wait <target> [timeout]` | The verification primitive |
| `device_get` | `get text\|attrs <target>` | Precise state reads |
| `device_button` | `home` / `back` / `back --system` | Device-level navigation |
| `device_close` | `close [app]` | Ends the engine session |

Engine resolution order (`resolve_engine_binary`): `UNPEEL_DEVICE_BIN` env →
`~/.unpeel/device/bin/agent-device` → PATH via `setup::search_dirs()` (login
shell + common bin dirs). No bundled-sibling candidate, ever.

## Grants and injection

Mirrors the browser exactly, with the opposite default:

- `state.rs`: `DeviceAccess` (`off`/`on`), `AppState.device_access`
  (deviations-only map), `AppState.device_default_access` — **default Off**.
  The toolchain is usually absent, and driving a simulator or a physical
  phone is a heavier capability than a sandboxed browser tab.
- Per-call live gate in `device_mcp.rs::load_security` (re-reads
  app-state.json): revokes and default changes apply immediately.
- Launch injection: `SessionHostLaunch.device_mcp_enabled` → manifest
  `device_client_registered`. Claude: third additive
  `--mcp-config ~/.unpeel/mcp/claude-device-mcp.json` (rewritten every
  launch). Codex: wrapper `-c mcp_servers.unpeel-device.*` gated on
  `UNPEEL_DEVICE_MCP_BIN`.
- Restart banner token `device-access:on` (explicit overrides only), grant
  carried across restart, pruned with the session.
- Native UI: sidebar right-click ▸ Device Access (Claude/Codex sessions);
  Settings ▸ Devices (`DeviceSettingsPanel`): engine + Node status, default
  picker, deviations list.

## Verification

- `cargo test` (device_mcp unit tests: schemas, arg validation, defaults).
- `swift test --package-path apps/native/UnpeelNative`
  (`testDecodesDeviceAccessWithFallback`).
- `apps/native/verify-device.sh` — grant gating + engine-through-MCP; skips
  gracefully without Node/agent-device. `--full` boots an iOS simulator and
  runs open → snapshot refs → screenshot artifact → wait → close.
- Live-verified 2026-07-03 on agent-device 0.18.3 / Xcode 26.4: full loop
  against the iOS Contacts app (open, snapshot with refs, tap `@e10`,
  wait-for-text, screenshot into the session artifact dir, cleanup).

## Deferred by choice

- Video/log/trace/network/perf evidence tools (the engine supports them; add
  as demand appears — each is one more argv builder).
- Android live verification (no Android SDK on the dev machine yet).
- `device_find` / `device_is` assertion tools.
- Wizard exposure: the setup wizard doesn't mention devices (default Off).
- iOS remote surfaces for device artifacts (`/api/sessions/:id/artifacts/…`
  currently routes only `browser`).
