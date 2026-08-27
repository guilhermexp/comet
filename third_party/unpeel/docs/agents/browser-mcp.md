<!-- Split out of the repo-root AGENTS.md (2026-08-05). The root AGENTS.md holds the map, hard rules, and invariants; this file is the full detail for its topic. -->

## Built-in Browser MCP (Browser Access)

Unpeel gives an agent session a real browser through the `browser` domain of its
first-party MCP server. Design rationale and verified engine findings:
`docs/feature/browser-mcp-deep-check.md` (the engine has **no MCP mode of its
own** — Unpeel authors the server and owns the tool schema).

> **Security scope:** the separate browser profile isolates browsing data from
> the user's normal browser; it does not isolate the hosted process. On/Ask/Off
> and site rules are cooperative controls for agents using the browser tools, not a
> sandbox against commands running as the same OS user. Same-user code can read
> local Unpeel state and invoke local tools outside this wrapper. Do not call
> these settings a hard security boundary.

- Implementation: `crates/unpeel-core/src/browser_mcp.rs`, served exclusively by
  the unified host `unpeel-host __mcp__` (`mcp_host.rs`) as the single `browser`
  action-enum tool. Caller identity via `UNPEEL_SESSION_ID`. 13 actions (`open`,
  `snapshot`, `click`, `fill`, `type`, `press`, `get`, `screenshot`, `wait`,
  `scroll`, `console`, `close`, `context`), each translated into one CLI
  invocation of the bundled `agent-browser` engine by `run_tool`. The host builds
  the argv itself, so agents can never pass policy-overriding flags. `context` is
  always callable (it explains access state); the rest are gated per call. The
  pre-unification flat `browser_*` tool names still resolve unadvertised
  (`mcp_host::run_tool`) for stale clients.
- Engine: `agent-browser` in its experimental `--native` mode
  (`AGENT_BROWSER_NATIVE=1`) — a pure-Rust CDP daemon driving the **system
  Chrome/Chromium**, no Node/Playwright/Chromium download. Verified live: open,
  snapshot refs, screenshot, allowed-domains enforcement. Binary resolution
  (`resolve_engine_binary`): `UNPEEL_BROWSER_BIN` env → sibling of `unpeel-host`
  (packaged layout) → `~/.unpeel/browser/bin/agent-browser` → PATH (dev). Each
  Unpeel session gets an isolated engine session `unpeel-<session-id>` (own
  daemon, profile, headed window), sockets under `~/.unpeel/browser/sockets`.
- Artifacts: with the default Settings ▸ Sessions use auto-gallery toggle,
  screenshots land in
  `~/.unpeel/app-sessions/<id>/artifacts/browser/screenshots/`; when disabled,
  ordinary captures land in the unlisted `.../browser/captures/` directory
  until the agent calls Sessions `add_to_gallery` (or requests the screenshot
  with `gallery: true`). Downloads use `.../downloads/` via
  `AGENT_BROWSER_DOWNLOAD_PATH`; tools return their paths. Phone screenshot
  requests explicitly set `gallery: true`.
- Grants (`state.rs`, reworked 2026-07-18): `BrowserAccess` is now
  `off`/`ask`/`on` — the same three-mode picker as computer use, with **On
  ("Allow") as the default** (the engine uses an isolated per-session profile
  with no access to the user's own logins, so the browser adds visibility
  rather than privilege; Settings ▸ Browser ▸ Off is the master disable).
  Under `ask`, a session's first browser action blocks on an approval alert
  (`/mcp/approve-browser`, `MCPBrowserApproval.swift`); Allow is remembered
  in `browser_approvals` with the same prune/carry lifecycle as
  `computer_approvals`, revocable in Settings ▸ Browser. `On` serializes as
  `"on"` for wire compat; `from_state_str` accepts `"allow"` as a synonym.
  Browser MCP is also **experimental** in the native app
  (`ExperimentalFeature.browserMcp`, env `UNPEEL_DEV_BROWSER_MCP=1`), gating
  the Settings ▸ Browser tab and native launch injection. Headless/TUI/CLI
  launches have no native UserDefaults feature layer, so they derive launch
  injection directly from the shared `browser_default_access` setting. There
  is still **no per-session override map** (the legacy `browser_access`
  deviations map is decode-tolerated, never written). Elevated future modes
  (shared/copied Chrome profile, live CDP) must stay opt-in. The server
  re-reads access and approvals per call, so changes apply live.
- Injection is per-session at launch (`SessionHostLaunch.browser_mcp_enabled` =
  native experimental flag on, where applicable, &&
  `browser_default_access != off`; malformed explicit access fails closed),
  recorded as the `browser_mcp_enabled` domain grant; the separate
  `browser_client_registered` bit records automatic provider setup. Since 2026-07-18
  the browser tools ride the **unified `unpeel` server** (the `browser` action
  tool, advertised only when the domain grant is set — see the unified-surface
  note in the Sessions MCP section); there is no separate per-provider browser
  config and no standalone browser server.
  Flipping the app-wide default **on** takes effect in a newly configured
  terminal (there is no per-session reload banner); **off** applies live
  through the per-call gate.
- Lifecycle: the engine daemon + Chrome deliberately outlive the provider CLI,
  so `UnpeelStore.killAndCleanup` also spawns
  `unpeel-host __browser_cleanup__ <id>` (closes the daemon, removes its
  socket/pid files). Browser access is app-wide, so a restart just relaunches
  under the same global default — no per-session grant to carry.
- Engine options (`AppState.browser_settings`, `BrowserSettings` in
  `state.rs`): `headed` (default true — visible window; false = headless,
  screenshots still work), `allowed_domains` (engine-enforced allowlist with
  wildcards; blocks navigation, sub-resources, and WebSockets), `profile_mode`
  (`"session"` = fresh profile per session; `"project"` = persistent
  Unpeel-managed profile per project tree under `~/.unpeel/browser/profiles/`
  so logins survive across a project's sessions — never the user's own Chrome
  profile), `executable_path` (custom Chromium-based browser; empty =
  auto-detect Chrome), `show_cursor` (default true — before each
  click/fill/type-into-target the server injects a fixed-position pointer
  overlay into the page and glides it to the target's center via `get box
  --json` + `eval`, so a human watching the headed window can follow the
  agent; strictly best-effort and headed-only, `maybe_show_cursor` in
  `browser_mcp.rs`). All read per engine invocation (`load_options` in
  `browser_mcp.rs`) so Settings changes apply to the agent's next browser
  action with no restart. The server also passes the app's `theme`
  (dark/light) as the page color scheme and `AGENT_BROWSER_MAX_OUTPUT`.
- Native UI: Settings ▸ **Browser** only (engine status probe, the single
  app-wide access picker, Options — window/browsing data/clear/browser app,
  Site access rules) in `SettingsView.swift` (`BrowserSettingsPanel`). There is
  no sidebar Browser Access menu. `UnpeelStore.setDefaultBrowserAccess` /
  `updateBrowserSettings` / `clearBrowserProfiles` are the write paths.
- **The Node "full engine" mode is ruled out** (product decision 2026-07-02):
  Unpeel stays a lightweight Swift app and will never ship or require a Node
  runtime. Everything that lives only in the engine's Node/Playwright daemon
  — video recording (native `record` silently writes no file), traces,
  viewport WebSocket streaming (`AGENT_BROWSER_STREAM_PORT` is a no-op in
  native mode), and the iOS Simulator path — is therefore "needs an upstream
  native-daemon contribution" (CDP `Page.startScreencast` is the natural PR),
  never "detect/enable Node". Deferred by choice, not blocked: action
  policies/confirmations, the engine auth vault, per-origin header injection,
  extensions.
- Debugging: `browser-mcp` lines in `~/.unpeel/hooks/trace.log`. Test with
  `printf '...' | UNPEEL_SESSION_ID=<id> unpeel-host __mcp__` calling the
  `browser` tool, or end to end via `apps/native/verify-browser.sh`.
- Bundling: `build-app.sh` copies the engine to
  `Unpeel.app/Contents/MacOS/agent-browser` (next to `unpeel-host`, the first
  resolution candidate), re-signs it, and ships the Apache-2.0 notice in
  Resources. Source order: `UNPEEL_AGENT_BROWSER_BIN` env →
  `~/.unpeel/browser/bin/agent-browser` → npm global (JS shim resolved to the
  darwin native binary). Missing engine is a non-fatal note in dev builds;
  treat it as a release blocker until the engine is vendored deterministically.
- Kept-per-project **logins** persist via the engine's state save/restore
  (`AGENT_BROWSER_SESSION_NAME=unpeel-proj-<root>` →
  `~/.agent-browser/sessions/*.json`), NOT via the Chrome profile dir: the
  native daemon launches Chrome with `--use-mock-keychain`, so encrypted
  cookies in the profile are purged on every restart (see the deep-check
  doc's native-mode findings). The profile dir still persists
  localStorage/IndexedDB/cache. Do not re-attempt cookie-DB copying
  ("seed from Chrome" was built and removed 2026-07-02 for this reason).
- **Shared-profile singleton conflict (fixed 2026-08-13):** Chrome enforces a
  process singleton per user-data-dir, and the kept-per-project profile is
  shared by every session in the project — so when another session's live
  browser owns it, a second launch hands the URL to *that* browser (a window
  pops up in the wrong session) and exits without a DevTools URL ("Chrome
  exited before providing DevTools URL"). `resolve_profile_dir` in
  `browser_mcp.rs` detects a live foreign lock owner before each launch and
  falls back to a per-session sibling dir (`<project>--s<session8>`); logins
  are unaffected because they ride the shared `SESSION_NAME` state, not the
  dir. It also restarts this session's daemon when it is alive but
  browserless (a daemon that started while the profile was busy has the
  doomed dir baked into its env forever), and evicts an orphaned Chrome on
  the session-scoped fallback dir — kills only with argv identity proof,
  never a bare recorded pid. `__browser_cleanup__` GCs the fallback dir on
  session close when no live Chrome owns it.
