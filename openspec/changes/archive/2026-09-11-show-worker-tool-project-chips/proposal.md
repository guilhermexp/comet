## Why
Workers tool headers expose opaque project IDs although the local catalog has the project name. Show a recognizable mention chip matching the composer.

## What Changes
Resolve exact local project IDs for Workers tool headers and show action plus @name. Preserve raw payloads, unknown IDs, session targets and remote-device calls.

## Capabilities
### New Capabilities
- `worker-tool-project-chips`: Named project targets in Chat tool headers.

## Impact
UI transcript, AppState and Workers snapshot publication only; no wire or execution changes.
