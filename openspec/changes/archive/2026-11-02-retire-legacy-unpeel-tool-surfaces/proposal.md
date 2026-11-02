# Proposal

## Why

Comet-launched Workers still reach two legacy engines of the vendored Unpeel MCP host and a set of obsolete tool aliases:

- **F1** `unpeel.browser` defaults to enabled for new Workers when the inherited config is missing, is advertised without checking that its engine exists (the owner's own test found the engine absent), and does not drive Comet's internal Browser. It is a phantom capability.
- **F3** `unpeel.computer` is the legacy cua-driver backend. New Workers already default it off, but the dispatch, `context`/`help` paths, legacy-manifest grants and the `__computer_cleanup__` route remain reachable. Comet's real Computer Use is `comet-computer-use` with its own authorization and device semantics.
- **F5** The peer host still accepts unadvertised legacy names (`browser_*`, `list_sessions`, standalone `read_transcript`, `inspect_session`, `send_text`, …) and the familial aliases `list_children`, `wait_for_children`, `summarize_children`, `report_to_parent`, which now mean peer-group operations and mislead agents.

## What Changes

- **F1/F3:** A Comet-only policy registered by the Comet binary before any vendored entry point runs retires the legacy Browser and Computer domains for Comet: new and resumed Workers launch without those grants (legacy manifests included), the Comet `__mcp__` host neither advertises nor dispatches them (no `help`/`context` exemption), and the Comet binary no longer routes `__browser_cleanup__`/`__computer_cleanup__`. The Sessions grant stays on so `comet-computer-use` and other extra servers keep being registered for Claude/Cursor/Cline. Standalone Unpeel defaults and its engines are unchanged.
- **F1 consumer migration:** there is no Comet agent-facing browser tool to migrate to; agents keep their providers' own browser tools and the user's internal Browser. Removal is the migration — no translation into Computer Use, no phantom advertisement.
- **F5:** Remove every callable legacy alias from the peer host. Canonical surfaces remain: `sessions(action: …)` including `read_transcript`, `list_group`, `wait_for_group`, `summarize_group`, `report_to_group`, and `browser(action: …)` where a standalone build enables it. An unknown or removed name fails without side effects and names the canonical replacement where one exists. Help, errors and docs stop teaching removed names. Internal dispatch keys may keep old spellings privately.

## Capabilities

### New Capabilities

- `comet-worker-tool-surface`: what the vendored MCP host exposes to Comet-launched Workers, and the legacy names it refuses.

## Impact

- Vendored `third_party/unpeel/crates/unpeel-core` (`mcp_host.rs`, `session_ops.rs`, `session_host.rs`, a small policy module) with `third_party/unpeel-upstream.toml` provenance updated in the same commit.
- `crates/workers-unpeel` (entry-point dispatch, extra registration), docs and AGENTS contracts.
- Active CLI processes are not restarted to reload catalogs; new launches and resumes pick up the reduced surface at their next boundary. Stale cached calls fail with a removal message.
