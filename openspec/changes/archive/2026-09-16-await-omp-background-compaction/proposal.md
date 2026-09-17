## Why

The installed OMP dispatches `/compact` in the background and returns `agentInvoked:false` before it finishes. Comet's immediate state request reads stale context and can close the process before the terminal command output arrives. The UI also discards valid counts when compaction increases context.

## What Changes

- Await the terminal compaction command output rather than its prompt ACK, with cancellation and a finite deadline.
- Refresh context before completion; update the gauge without a subsequent model turn.
- Show before/after counts for decreases, increases and unchanged counts; remove the UI's delayed state read.
- Preserve existing generic fallback when the runtime omits usage and report command failures honestly.

## Capabilities

### New Capabilities
None.

### Modified Capabilities
- `omp-compaction-tokens`: background completion and immediate context publication.

## Impact
OMP adapter and its subprocess fixtures, UI compaction marker, owner docs. Existing wire types remain unchanged.
