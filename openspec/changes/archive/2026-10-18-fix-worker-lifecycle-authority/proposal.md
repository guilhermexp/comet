# Proposal

## Why

A live OMP Worker was marked idle and rebound to an embedded Claude conversation because globally installed hooks inherited the Worker's environment. Its subsequent interactive question produced no lifecycle event. All runtime integrations need the same authority and recovery rules rather than another OMP-only patch.

## What Changes

- Identify the reporting runtime in every managed shell hook and JavaScript extension/plugin; reject a different runtime before changing activity, provider identity, telemetry, journal, or parent notifications.
- Preserve event identity in durable recovery seeds and apply the same admission decision to HTTP, preflight, activity recovery, and journal recovery.
- Make shared and provider-specific reporters honor listener rejection before durable publication, including asynchronous delivery, while retaining recovery when listeners are unavailable.
- Audit lifecycle signals for all 16 catalog runtimes, correct genuine start/end/question/approval transitions, and remove unsupported event subscriptions or reliability claims. Hookless AGY remains output-derived.
- Deliver accepted attention events through the existing parent notification path, including an idle parent; ordinary question resolution resumes work without reporting completion.
- **BREAKING:** Prime Agent targets the current Rust 0.10 runtime, replacing the obsolete TypeScript-extension launch path with genuine native lifecycle observation while preserving its interactive terminal and explicit-session resume.

## Capabilities

### New Capabilities

- `worker-lifecycle-authority`: provider-neutral lifecycle ownership, durable recovery, and attention delivery across the catalog.

### Modified Capabilities

- `pi-worker-lifecycle-hooks`: replace unsupported prompt event assumptions with provider-supported interaction signals, retaining nested OMP isolation.

## Impact

Vendored runtime reporters/adapters/descriptors and shared hook assets, Comet Worker activity/journal/parent-notification integration, behavioral regressions, downstream DOX and vendor provenance. No Chat CRDT or remote Box integration changes. Existing running extension hosts may need to restart to load updated JavaScript; this change does not restart or mutate the user's live sessions during verification.

The user explicitly authorized fixing all runtimes; these artifacts document that implementation rather than request a second approval.
