# Change: Harden Chat sync backpressure

## Why

When the Chat sync server rejects pushes for exceeding the per-device quota, new local updates can bypass the retry deadline and replay the full pending queue. This amplifies traffic precisely while the server is asking the client to slow down and obscures recovery behind a flood of repeated warnings.

## What Changes

- Treat a quota rejection as a bounded backpressure state for that Chat connection.
- Queue local updates during the cooldown without sending them immediately.
- Retry only the blocked head update after the cooldown and resume ordered draining on acknowledgements.
- Make the canonical `zeron-sync` test command compile its integration-test support consistently.

## Capabilities

### New Capabilities

- `chat-sync-backpressure`: bounded, ordered recovery when a Chat sync push is rejected for quota.

### Modified Capabilities

None.

## Impact

- `crates/sync/src/chat_client.rs` and its tests.
- `crates/sync/Cargo.toml` and the registry integration-test gate.
- No edge quota, protocol, updater, release, version, or upstream change.
