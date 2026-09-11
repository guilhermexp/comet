## Why
Workers opens the platform folder dialog while Orchestrator uses the in-app project palette. The inconsistent flow loses shared search, keyboard navigation and locations.

## What Changes
Reuse the existing project palette for Worker project entry points and Command-K in Workers. Restrict Worker browsing and registration to the local device; keep Orchestrator multi-device behavior.

## Capabilities
### New Capabilities
- `workers-project-picker`: shared folder selection with destination-specific registration.
### Modified Capabilities
None.

## Impact
UI shell project palette and Workers sidebar; no wire, sync or runtime lifecycle changes.
