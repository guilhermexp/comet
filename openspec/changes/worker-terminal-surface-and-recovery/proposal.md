## Why
Workers adds a terminal background over the shared Orchestrator canvas. The utility divider includes Details in its width and responsive compression redistributes the resulting excess into the neighboring panel. Terminal resize requests can also overlap and UI geometry exceeds the host limits. A stopped Claude Worker exits its alternate screen, leaving an empty primary screen even though its output is retained.

## What Changes
- D1: Inherit the shared canvas for Workers, including loading and stopped states.
- D2: Make utility/Details dividers follow the painted geometry, preserve the neighbor and share the responsive layout limits. Serialize terminal resize requests and use the host cell limits.
- D3: Retain the last alternate screen for inspection only when a stopped Worker would otherwise be blank.

## Capabilities
### New Capabilities
- `worker-terminal-surface-recovery`: Consistent canvas, synchronized geometry and stopped screen recovery.

## Impact
UI shell columns, Workers, shared terminal emulator and renderer. No deletion, restart or archival of Workers; no credential or dependency changes.
