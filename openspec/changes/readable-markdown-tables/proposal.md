## Why
Markdown tables compress verdict words and inline chips into narrow columns. Chip decoration is missing from the width calculation.
## What Changes
- Measure inline boxes and longest words when determining column minimums.
- Keep prose columns readable and constrain overflow to the horizontal table viewport.
## Capabilities
### Modified Capabilities
- `turn-step-tool-groups`: readable Markdown tables in agent output.
## Impact
Native Markdown table layout only.
