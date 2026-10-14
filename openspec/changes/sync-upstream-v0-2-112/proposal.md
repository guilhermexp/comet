# Proposal

## Why

Resolve fork PR #23 against the fixed upstream v0.2.112 snapshot by a real merge, preserving the fork's product contracts. Delivery is local preparation only under ticket WT-20261014-comet-integra-upstream-v0-2-112-pr-23-preservando-contratos.

## What Changes

- Integrate the 14 commits in `54954e4d22bd4d78b2e2c7160a913b1af548dc79..75b72602a0348f5697c7affc9f84afe367b763fb`: projectless scratch folders and bounded home search, per-device worktree locations, persistent browser profiles, PR side panes, adaptive Session information, multiselect Files, overscroll, idle harness icons, usage reset tooltips, iOS grouping, MCP worker limits and macOS entitlements/AutoFill.
- Preserve Workers/OMP/Kimi, root-only MCP grants, Enter/Steer, Details/Usage, right-side terminal, typed RPC registry, package version 0.2.18 and own-feed-only updater.
- Keep the refused macOS workflow deleted; port relevant storage verification into the fork workflow without replacing its gate.
- Record every conflict decision, incoming-line audit and final gate evidence; do not publish or install.

## Capabilities

### New Capabilities

- `fork-upstream-refresh`: accepted upstream behavior remains available without overriding fork security, placement and delivery contracts.

### Modified Capabilities

None; the integration-specific boundary follows the previous sync's delta convention.

## Impact

Engine/proto/RPC/client, desktop UI, iOS, macOS bundle metadata and packaging scripts, fork workflows and sync history. No Edge delta, tag, main merge, push, app build or installation. Graft is unavailable in this checkout (no executable or graph); exact Git incoming deltas provide discovery evidence.
