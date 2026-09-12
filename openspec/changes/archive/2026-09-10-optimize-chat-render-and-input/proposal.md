## Why

Streaming still repeats transcript copies, Markdown block allocations, grammar compilation and composer shaping. Adapt the upstream P1/P3 improvements to reduce this work without losing the fork's interaction fixes.

## What Changes

- Share immutable Markdown blocks and compiled grammars, borrow transcript entries and bound paint caches.
- Avoid transcript derivation for unrelated state updates while preserving all visible status changes.
- Cache composer layout, wrap attachment rows, and recover mounted keyboard focus.
- Suspend streaming follow when text selection starts.

## Capabilities

### New Capabilities

### Modified Capabilities

- `composer-intake`: wrapped staged and sent attachment rows without clipping.
- `turn-step-tool-groups`: selection stability and interaction continuity during streaming.

## Impact

Native Rust UI and syntax crates only. No protocol, dependency, renderer pin, engine, deployment or theme changes. Based on upstream PRs #255, #271, #274, #285 and selection portion of #265; adapted to local previews, Gray decoration, tool grouping and OMP behavior.
