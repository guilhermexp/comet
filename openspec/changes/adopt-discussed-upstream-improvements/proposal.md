## Why

Complete the P1–P9 adoption the user requested from the 2026-09-10 upstream comparison. P1/P3 landed in `b2e41d4c`; the remaining priorities address idle repaint work, missing causal sync history, premature ACP completion, remote Files, development server previews, Git history controls and iOS interactions.

## What Changes

- P2: filter presentation-equivalent presence updates; adopt the reviewed macOS renderer lifecycle and allocator improvements while preserving the fork's wrapping, native previews, transparency, blur and input behavior.
- P4: hold the persisted cursor when imported Chat operations lack causal dependencies and recover through checkpoints on WebSocket and HTTP paths.
- P5: keep ACP prompts alive through silence until an authoritative end, interruption or transport failure; preserve structured JSON-RPC error details.
- P6: use bounded directory RPCs, pagination, watching and incremental reconciliation for local/remote Files in the existing sidebar and native preview.
- P7: discover and open development servers in a native browser, with stable preview URLs, HMR and authenticated cross-device transport.
- P8: add Git history search, branch-tip navigation and configurable columns to the existing history panel and ProcessRunner boundary.
- P9: bring the reviewed iOS streaming, keyboard, scrolling, composer-clear and tool-group fixes.
- Preserve steering/Live Voice, OMP and Workers, Chat code wrapping without horizontal scroll, Gray decoration, native file previews and absolute external local file links. Do not import the rejected queue-only behavior, full editor replacement, release/deploy workflows or telemetry.

## Capabilities

### New Capabilities

- `desktop-renderer-efficiency`: idle presentation invalidation and wake-safe native rendering.
- `chat-causal-recovery`: durable cursor truthfulness and causal checkpoint repair.
- `acp-prompt-lifecycle`: authoritative prompt completion and structured errors.
- `remote-files`: bounded local/remote directory browsing in the existing Files surface.
- `dev-server-preview`: discovery and native local/cross-device development previews.
- `git-history-controls`: search, branch-tip navigation and configurable history columns.
- `ios-chat-interactions`: mobile streaming, input, scrolling and tool disclosure behavior.

### Modified Capabilities

## Impact

The source baseline is the discussed `upstream/main@a1adfde2` (v0.2.59), not every newer upstream commit fetched later. Integration is isolated in `feat/upstream-adoption-20260910`, based on local `b2e41d4c`. Affected boundaries are syntax/UI/GPUI, sync/engine, harness, proto/RPC, optional preview crate/edge and apps/ios. New wire fields remain optional/defaulted; older devices must degrade explicitly. Local validation and commits are authorized; no remote publication, deployment, signing or release is part of this adoption. Cargo runs serially with at most two build jobs; BCU QA runs separately.
