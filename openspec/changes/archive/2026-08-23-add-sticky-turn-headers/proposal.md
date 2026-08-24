## Why

Comet currently positions a newly sent user message at the transcript top with an own-turn runway, but releases that position as soon as the user scrolls. Long assistant output therefore loses the prompt context, while Orchestrator.dev keeps each user message as the sticky header of its own turn.

## What Changes

- Derive the user turn crossing the transcript reading line from the virtualized row projection.
- Repaint that turn's existing user row renderer as a layout-neutral sticky header after the original row crosses the top inset.
- Bound the sticky header by the next user row so the next turn pushes and replaces it.
- Preserve the own-turn runway for new-message arrival and hand off between the runway and historical sticky behavior without a jump or duplicate.
- Cache measured user-card geometry per chat so bottom-glued and remeasured lists select the visible group correctly.

## Capabilities

### New Capabilities

- `sticky-turn-headers`: per-turn sticky user-message context for the virtualized native transcript.

### Modified Capabilities

None.

## Impact

- `crates/ui/src/transcript.rs`: turn-boundary derivation, geometry cache, overlay rendering, runway handoff, and deterministic tests.
- `crates/ui/AGENTS.md`: durable transcript contract and test coverage count.
- No protocol, schema, runtime adapter, dependency, composer, or persisted-state change.
