# Change: Repair native runtime integrity

## Why

The native app logged GPUI reentrant `RefCell` borrow failures and warned while observing an incomplete Chat Transcript part during incremental CRDT import. Both paths let the app continue, but the first needs a reproducible interaction trace and the second made a valid transient state look like durable corruption.

## What Changes

- Reproduce and remove the reentrant GPUI update path without weakening borrow checks.
- Add privacy-safe structural diagnostics that identify the malformed transcript field path and part kind without logging content.
- Distinguish contentless incremental-import shells from durable malformed parts without weakening the strict schema.
- Treat a TCP peer that disconnects before completing the local IPC WebSocket handshake as expected debug noise while retaining warnings for malformed or rejected handshakes.
- Treat a checkout without a resolvable GitHub identity as an expected unsupported-provider state while retaining warnings for actionable GitHub failures.
- Make peer credential-revocation subscription synchronous with cache construction so sign-out cannot leave authenticated cached sockets alive.
- Retain salvage as a defensive reader path and add regression coverage for both transient and actionable malformed structures.

## Capabilities

### New Capabilities

- `native-runtime-integrity`: non-reentrant native interaction dispatch and privacy-safe Chat Transcript recovery diagnostics that understand incremental CRDT import.

### Modified Capabilities

None.

## Impact

- `crates/ui` native interaction callbacks and focused UI tests/harnesses.
- `crates/doc/src/schema.rs` diagnostics and salvage regression tests.
- `crates/doc/src/schema.rs` transient-import classification and regression tests.
- `crates/rpc/src/server.rs` local IPC handshake diagnostic classification.
- `crates/rpc/src/device_room.rs` peer-link credential revocation.
- `crates/engine/src/change_requests.rs` unsupported-provider diagnostic classification.
- No updater, release, version, or upstream change.
