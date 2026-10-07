# Proposal

## Why

A pending CLI update defers every Steer until the parent turn ends. Worker notifications use Steer too, so a parent waiting for a Worker can never receive the notification needed to finish that turn. Turn-boundary steering also puts these notifications in the composer queue, contrary to their existing purpose of waking the Orchestrator.

## What Changes

- Deliver genuine Worker notifications directly to an existing steerable parent run, including when a CLI update is waiting for that run.
- Preserve ordinary message queueing, exclusive installation leases, notification deduplication and durable fallback when the parent has no steerable run.
- Prove the delivery regression through the engine and retain the OMP bridge contract that interrupts `wait_for_status` before delivering the notification.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `workers-host-bridge`: Worker notifications bypass composer waiting and pending CLI updates when the parent can receive them directly.

## Impact

Engine routing in `sessions.rs` and `doc_host.rs`, focused engine tests, and the engine DOX contract. No new wire shape or dependency. The multi-device scope of Update all is a separate concern.
