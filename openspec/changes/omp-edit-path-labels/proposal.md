## Why
OMP hashline edit inputs carry paths in tagged section headers, while the adapter only reads path. This produces blank Edited cards.
## What Changes
- Derive edit targets from canonical hashline headers when explicit path is absent.
- Recover legacy card labels from recorded output headers where available.
- Never render an empty file label or open an unknown/multiple target as one file.
## Capabilities
### Modified Capabilities
- `turn-step-tool-groups`: edit target identification.
## Impact
Proto pure helper, OMP normalization and native edit labels. Wire shape remains unchanged.
