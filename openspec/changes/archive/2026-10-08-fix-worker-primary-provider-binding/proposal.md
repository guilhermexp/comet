# Proposal

## Why

OMP subagents inherit the Worker's lifecycle hook endpoint and can overwrite its provider binding. The observed Workers row therefore displayed PgliteSqlCheck's Gemini model and tokens while the primary Worker was using Opus 5.5.

## What Changes

- Bind OMP Worker telemetry to the primary conversation; nested subagent conversations cannot replace that binding or drive the primary lifecycle.
- Recover an existing incorrect nested binding when primary conversation evidence arrives.
- Preserve provider path validation, bounded parsing and optional telemetry fallback.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `workers-widget-model-usage`: distinguish primary Worker provider evidence from nested subagent evidence.
- `pi-worker-lifecycle-hooks`: exclude nested subagent lifecycle events from primary Worker state and binding.

## Impact

OMP lifecycle extension, Worker hook ingress/provider binding, targeted regression tests and owning DOX contracts. No wire schema or UI presentation change is required.
