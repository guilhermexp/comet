## Why
Intermediate command/read totals interrupt the streaming timeline. The user wants counts only in the completed turn summary.

## What Changes
Render individual calls directly for every tool group, in streaming and expanded completed turns; retain only the outer TurnSteps total. Preserve individual details, state and ordering.

## Capabilities
### Modified Capabilities
- `turn-step-tool-groups`: only the settled turn has an aggregate counter.

## Impact
Native transcript group disclosure policy; no durable data changes.
