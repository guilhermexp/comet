## Why

OMP local slash commands finish without refreshing context usage. After `/compact`, the existing Chat marker therefore reads the old token count and falls back to “Context compacted.” instead of showing the reduction.

## What Changes

- Refresh OMP context usage before completing successful local slash commands, using the same finalization as regular turns.
- Preserve graceful completion when the runtime cannot report usage.
- Add subprocess regression coverage for compaction and absent usage.

## Capabilities

### New Capabilities
- `omp-compaction-tokens`: Refresh context usage after local compaction so the Chat can display the before/after token counts.

### Modified Capabilities

None.

## Impact

OMP harness and RPC fixture tests. Existing UI marker, wire types and persistence stay compatible.
