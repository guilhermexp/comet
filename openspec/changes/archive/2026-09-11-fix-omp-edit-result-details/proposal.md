## Why

OMP edit results carry authoritative file snapshots in `result.details`. The adapter reads only top-level fields, leaving input-string edits as an empty `Edited` card without a filename or diff.

## What Changes

- Read single-file snapshots from OMP result details while preserving the legacy top-level format.
- Feed the existing ToolDiff/document/file-card pipeline; do not invent snapshots when omitted or combine different files into one diff.

## Capabilities

### New Capabilities
- `omp-edit-result-details`: retain authoritative single-file edit metadata in the Chat Transcript.

## Impact

OMP normalizer and regression tests. No wire, renderer, or storage schema changes.
