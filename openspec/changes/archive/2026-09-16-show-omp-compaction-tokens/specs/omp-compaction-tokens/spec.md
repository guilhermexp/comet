## Purpose

Make the context token reduction from OMP local compaction available to the Chat completion marker.

## ADDED Requirements

### Requirement: Refresh usage before local command completion
The OMP adapter SHALL publish available post-command context usage before reporting a successful local slash command complete. The existing Chat compaction marker SHALL be able to use that updated count alongside its pre-command count.

#### Scenario: Compaction reduces context
- **WHEN** `/compact` completes locally and the runtime reports 63000 context tokens
- **THEN** the adapter publishes 63000 context tokens before exactly one successful completion, enabling the existing before/after marker
- **Test:** integration — `crates/harness/tests/omp_rpc.rs`

#### Scenario: Usage is unavailable
- **WHEN** a local command succeeds but its state has no usage or its state request fails
- **THEN** completion still succeeds exactly once without fabricated token counts or duplicate command output
- **Test:** integration — `crates/harness/tests/omp_rpc.rs`
