## Why

The owner requested initial task delivery when launching a worker, regardless of preset. The OMP-only implementation was explicitly rejected as a partial delivery. The configured catalog contains OMP, Claude, Pi and Codex. All four must receive initial_text through launch itself; proving only one runtime is not acceptance.

## What Changes

- Generalize native initial-task delivery to all four configured runtimes and presets selecting those runtimes.
- Use each CLI's supported startup input without changing interactive mode, model selection or permission flags.
- Preserve the one-shot reservation, honest delivery receipt, parent task association, checkout and restart-without-replay behavior.
- Keep task content out of persisted restart commands, shell history and diagnostic argv logs; preserve literal text and trailing newlines.
- Prove every preset through the installed Comet MCP, without later task submission. Existing authentication/permission gates remain real and must not be bypassed to manufacture a passing probe.

## Capabilities

### New Capabilities
- `worker-initial-briefing`: initial task delivery and truthful launch outcomes for OMP, Claude, Pi and Codex Workers.

### Modified Capabilities

None.

## Impact

`crates/workers-unpeel`, the vendored Unpeel launch/runtime boundary and its four integration adapters, including the Codex wrapper's argument logging; local DOX, provenance and this existing OpenSpec change. The public launch API remains unchanged. Orchestrator.dev is a read-only reference. Preserve unrelated WIP and running sessions. New retry orchestration, Graft repairs and the pre-existing restart response-ID race are not part of this change.
