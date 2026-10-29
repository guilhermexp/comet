# Proposal

## Why

An accepted OMP update rejects ordinary steering into a live runtime. Routed sends can then terminate that runtime, while turn completion also exits it despite remaining background work. Direct CLI updates do not execute these Comet admission paths.

## What Changes

- Keep OMP's active turn, accepted continuations and background work alive while an update waits.
- Admit compatible steers and routed sends to that process without acquiring another execution lease.
- Retire the idle runtime only after its accepted work drains; keep installation and subsequent execution exclusive.
- Preserve explicit Stop, other providers' admission policy, updater failure reporting and OMP RPC chunk diagnostics.

## Capabilities

### New Capabilities

- `agent-cli-updates`: Add OMP active-runtime safety requirements to the capability already proposed by `unify-agent-cli-updates` (not yet archived into main specs).

### Modified Capabilities

None in the archived inventory.

## Impact

Engine session admission and retirement, isolated updater/process regressions, and engine DOX. No wire changes, provider dependencies, voice resolver changes, real updates, or global gate redesign. Parent owns integration, Cargo verification, review gate and archive.
