# Proposal

## Why

A Worker recorded Stop at 00:48:38 while its sidebar still spun at 00:53. The desktop hook listener was absent from the capped broadcast registry, and the activity engine ignored durable events once latched, leaving the five-minute timeout as recovery.

## What Changes

- Recover newer durable lifecycle events on the next activity poll, retaining generation and event-order guards.
- Preserve registered listeners regardless of registration age; registration order is not evidence that a listener died.
- Add focused regressions for dropped delivery, stale events and more than sixteen listeners.

## Capabilities

### New Capabilities

- `worker-lifecycle-delivery`: timely lifecycle recovery and lossless listener registration.

### Modified Capabilities

None.

## Impact

Comet activity bridge, the shared vendored activity engine, and Rust/Swift writers of the hook-port registry. No wire format or runtime command changes. Direct implementation on main authorized by the owner.
