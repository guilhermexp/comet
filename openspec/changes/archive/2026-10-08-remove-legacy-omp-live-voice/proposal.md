# Proposal

## Why

The user has verified Codex Voice and requested deletion of their older OMP Live Voice integration in the Chat input. Keeping both exposes an obsolete microphone affordance and maintains unused lifecycle, protocol and delegation code.

## What Changes

- **BREAKING**: retire OMP Live Voice from the composer, engine, RPC registry and harness adapter, including its capability probes, transient state, controls and voice-specific delegation hooks.
- Remove obsolete tests, fixtures and capture knobs; preserve ordinary OMP text runs, durable steering, worktree creation and concurrent execution shutdown.
- Keep Codex Voice, its sidebar trigger, active-call return orb, standalone media helper, desktop/iOS lifecycle and microphone packaging declarations.
- Update canonical contracts and documentation to describe the remaining voice integration.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `omp-live-voice`: retire the capability and reject obsolete methods without mutating a Chat.
- `live-voice-shutdown`: remove OMP voice teardown requirements; retain concurrent independent run shutdown.
- `codex-live-voice`: remove the obsolete coexistence clause and preserve Codex host ownership.

## Impact

Rust UI, engine, harness, RPC and proto; their owner docs and focused tests/fixtures. Legacy local-only RPCs become unknown methods; no durable CRDT schema, stored Chat history, account data or device relay format changes. External OMP source and vendored runtime capabilities are outside this repository removal.
