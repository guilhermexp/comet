## Why
A generic task row repeats the delegation already represented by its linked subagents. Subagents also need a subtle individual backgrounds to distinguish each agent.

## What Changes
- Omit a non-failed task wrapper when recorded child IDs identify linked subagents.
- Keep wrappers without linked children and failed wrappers visible.
- Add a neutral translucent background, rounded corners and padding around each individual subagent, with no full-width group background.

## Capabilities
### Modified Capabilities
- `turn-step-tool-groups`: subagent group presentation.

## Impact
Native transcript projection and presentation only; durable parts and export are unchanged.
