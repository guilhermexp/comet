# Design

## Context

Source map: `.tmp/remote-input/desktop-corrections/SOURCE-MAP.md` (F1/F3/F5) and the consumer map recorded during implementation. Key facts:

- `session_ops::browser_mcp_enabled_from_app_state` returns `BrowserAccess::default()` (On) when the inherited state is absent; the launch sets `browser_mcp_enabled` from it and `computer_mcp_enabled: false`.
- `session_host` manifests persist `browser_mcp_enabled`/`computer_mcp_enabled` and fall back to `*_client_registered` evidence; resume-in-PTY reuses the manifest grant.
- `mcp_host` advertises browser when the domain mask allows it and the manifest grant is absent or true; `help`/`context` skip the grant check for both engines; legacy names are accepted by `legacy_sessions_action` and the `browser_` prefix.
- Comet runs every vendored entry point (`__mcp__`, host, cleanup routes) through `zeron_workers_unpeel` dispatch inside the Comet binary.

## Decisions

### D1 — One Comet-only policy, set at the vendored boundary

`unpeel_core::embedding::retire_legacy_engines()` (process-wide, set once) is called by Comet's dispatch before any vendored entry point. While set:

- launch and resume compute browser/computer grants as `false`, regardless of app state or legacy manifests;
- `mcp_host` treats the Browser and Computer domains as absent: no advertisement, no instructions mentioning them, and every call (including `help` and `context`) is refused with a removal message before any engine code runs.

Standalone Unpeel never calls it, so its defaults, its engines and its tests are unchanged.

### D2 — Remove the Comet-connected cleanup routes

Comet's binary stops routing `__browser_cleanup__` and `__computer_cleanup__`. The vendored modules stay for the standalone product.

### D3 — Alias removal is global in the vendored host

The aliases are agent-facing names, not standalone product features; removing them in the shared host is the requested cutover. `legacy_sessions_action`, the `browser_` prefix and the four familial names go. Old public names reach the generic unknown-tool path with a hint naming the canonical call, and nothing runs. `legacy_sessions_tool_definitions` remains only if used to render canonical help; strings that teach removed names are rewritten.

### D4 — Least privilege preserved

The Sessions grant and domain stay on for Comet Workers, so the unified `unpeel` server and `comet-computer-use` keep being registered for adapters that require `any_mcp()`.

## Risks

- **Running Workers with cached catalogs** keep stale names until their next launch/resume; calls fail with a removal message instead of retargeting.
- **Vendored drift:** provenance is updated in the same commit; the policy module is additive to minimize upstream conflicts.

## Verification

Consumer-visible unpeel-core tests: Comet policy launch/resume grants are off for new and legacy manifests; `tools/list` under the policy lists no browser/computer; `browser`/`computer` calls (including `help`/`context`) are refused without engine side effects; removed aliases are rejected without side effects while canonical actions still work; standalone defaults unchanged. Comet tests: entry-point dispatch no longer routes cleanup args. No Mac-only behavior is involved; adapter config writers are covered by existing conformance tests.
