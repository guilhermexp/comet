## Why

The fork is functionally based on upstream `v0.2.18`, while `upstream/main` now contains eleven releases through `v0.2.29` plus the post-release sticky-diff-header merge. Important registry/sync safety fixes, OpenCode runtime hardening, and desktop UX capabilities are therefore absent or only independently approximated in the fork.

## What Changes

- Integrate upstream history through `b3fa5187` in an isolated branch while preserving the fork's private product behavior, branding, local-first operation, updater policy, Workers runtime, Usage providers, transcript export, and developer tooling.
- Port registry and sync correctness fixes before visual features: cursor contiguity, server-truth orphan sweeping, retryable unreadable acknowledgements, future-harness row tolerance, and bounded diff reconciliation.
- Port the native OpenCode HTTP/SSE driver and connected-provider model discovery without regressing the fork's Workers/subagent event projection.
- Port upstream appearance, sidebar organization, Chat navigation shortcuts, model-picker scalability, composer popups, transcript/Thinking presentation, typography, and sticky Changes headers.
- Keep release tags, upstream publication endpoints, automatic update behavior, TestFlight publication, and upstream-only deployment configuration out of the fork's runtime behavior.
- Validate Rust, edge, packaging-sensitive paths, and native UI surfaces before the integration branch can be promoted.

## Capabilities

### New Capabilities

- `resilient-registry-sync`: Registry cursors, acknowledgements, orphan cleanup, forward-compatible Chat rows, and diff reconciliation remain safe under gaps and malformed responses.
- `native-opencode-runtime`: OpenCode runs through the bounded native HTTP/SSE path and exposes only connected provider models while retaining fork-specific Workers projection.
- `appearance-and-model-navigation`: The desktop app supports imported/custom themes, configurable interface typography, scalable harness-scoped model selection, and stable completion popups.
- `chat-navigation-and-sidebar`: Sidebar organization, sorting, archive/jump shortcuts, source context, and keyboard hints operate on durable Chats without confusing them with Sessions.
- `transcript-and-changes-navigation`: Reasoning/Thinking, transcript copying/viewport behavior, and sticky diff headers preserve the fork's richer transcript and Changes surfaces.

### Modified Capabilities

- `sticky-turn-headers`: Upstream transcript and Changes navigation must coexist with the fork's sticky live-turn header contract.
- `turn-step-tool-groups`: Upstream Thinking/tool-group behavior must preserve the fork's run-step grouping and subagent rendering rules.

## Impact

This touches the Rust workspace (`proto`, `doc`, `sync`, `harness`, `engine`, `ui`, and a new MIT-compatible `theme` crate), the edge registry protocol/tests, macOS/Linux packaging assets, and selected iOS compatibility code. Sixty upstream-touched paths overlap private fork changes, so reconciliation is semantic rather than a blind tree replacement. No upstream push, release tag, fork-main promotion, Cloudflare deploy, TestFlight upload, or updater enablement is authorized by this change.
