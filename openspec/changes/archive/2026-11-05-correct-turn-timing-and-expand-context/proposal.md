# Proposal

## Why

Details currently discards native OMP request durations and displays an inferred model time and rate: a recorded turn with 18 requests and 13,824 output tokens took 143.607892 seconds in native request durations, but the Session card displayed roughly 130 seconds and 106 tok/s. Users also cannot expand the Context indicator to understand which parts of the active session occupy the window.

## What Changes

- Carry native OMP assistant-message `duration` through the existing metrics pipeline and calculate the turn's model rate from that duration. Preserve the distinction between measured time and unavailable or inferred timing; partial measurements must not masquerade as a complete measured total.
- Add an expand/collapse control to the existing Details > Session > Context row. Preserve its compact appearance by default and reveal a segmented budget bar, used/capacity counts, category token counts and capacity percentages, expandable MCP-tool and memory-file lists, and measurement freshness.
- Cover the reference categories: messages, system tools, skills, system prompt, memory files, MCP server instructions, autocompact reserve, free space, deferred system tools and deferred MCP tools. Keep deferred material outside the active budget, avoid double counting, and explicitly distinguish absent breakdowns from real zeros.
- Use session-native data on the chat's host. Expanding the panel must not make a paid model request, inject a diagnostic prompt into the conversation, or consult a different chat's data. Runtime-specific availability and estimation must be visible instead of fabricated token counts.
- Extend OMP's RPC with a structured, local-only context inspection command backed by the live session's native accounting. Coordinate its versioned category/list contract with Comet; do not parse `/context` text, rebuild the provider prompt or force a new model request. This runtime work was explicitly selected by the user.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `details-sidebar`: Native model-time/rate accounting in Turn stats and an expandable, session-scoped Context breakdown with explicit availability and freshness.

## Impact

- OMP normalization and per-turn metrics aggregation in `crates/harness` and `crates/engine`.
- Coordinated OMP change in `/Users/guilhermevarela/Documents/Projetos/SelfHosting/oh-my-pi`, based on installed-version tag `v18.16.0`: session context accounting, RPC command/types and focused tests/documentation. The checkout is currently an unchanged upstream source baseline; publishing a fork or replacing the installed executable is a separate destination, not performed by this proposal.
- Optional context-detail types/events and the host-to-client data path in `crates/proto`, `crates/engine`, `crates/rpc` and `crates/client` as required by the native contracts.
- Session card rendering and existing per-context expansion preferences in `crates/ui/src/details_sidebar`.
- Focused runtime, engine, protocol and UI-state verification; the existing native QA safety restriction still applies.
- No dependency on the stats dashboard server or its SQLite index, no changes to subscription accounting, no release/deploy in this planning step, and no unrelated redesign of Session, Project or Context sources.
