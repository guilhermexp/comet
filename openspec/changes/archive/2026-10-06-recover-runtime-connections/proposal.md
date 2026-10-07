# Proposal

## Why

After system resume, active registry, Chat and device-room sockets ignore the wake signal until their receive lease expires. Registry shutdown also loses abort ownership if its await is cancelled and can wait through a pending dial or handshake.

## What Changes

- Reconnect transport sessions promptly on a system wake, preserving durable Chat outbox and registry replay.
- Cancel registry dial, handshake and blocked sends promptly on shutdown; retain task ownership across a cancelled shutdown future.
- Preserve remote-host offline cooldown evidence across a local system wake until fresh presence invalidates it.

## Capabilities

### New Capabilities

- `connection-recovery`: system-wake and shutdown lifecycle for registry, Chat and device-room transports, including remote-offline retry evidence.

### Modified Capabilities

None.

## Impact

`crates/sync` registry and Chat actors and `crates/rpc` device-room host/cache, their existing fake-transport tests and owning DOX contracts. No wire-format, schema or dependency changes. OMP startup/negotiation and auxiliary provider timeouts are investigated separately; transient DNS/provider unavailability and real quota rejection remain reported.
