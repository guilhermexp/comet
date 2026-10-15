# Proposal

## Why

The OMP runtime appended Comet's orchestrator system prompt (`--append-system-prompt`) only when the chat cwd was exactly `~/.orchestrator`. A chat opened in any other project ran OMP without the communication rules, the operational boundaries, or the delegation contract, even when the `workers` tool was registered. The owner wants this prompt applied by default in every workspace.

## What Changes

- Every OMP chat run appends the orchestrator system prompt, whatever its cwd.
- The delegation block keeps its own gate: it is added only when the `workers` tool will be registered (`workers_tool_expected`).
- The workspace gate (`OmpHarness::orchestrator_workspace`, `with_orchestrator_workspace`, `is_orchestrator_workspace`) is removed.
- Model and command discovery launches stay without the append.

## Capabilities

### New Capabilities

- `omp-system-prompt`: when the OMP runtime appends Comet's system prompt.

## Impact

- Code: `crates/harness/src/omp/mod.rs`.
- Tests: `crates/harness/tests/omp_rpc.rs`, `crates/harness/tests/fixtures/fake-omp-rpc.sh`.
- Docs: `crates/harness/AGENTS.md`.
