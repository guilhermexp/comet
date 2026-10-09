# Design

## Context

See proposal.md for the observed circular wait. Workers already emit durable Steer commands with a `worker-notify-message:` message id and a `[worker-task-notification]` prompt. The engine checks turn-boundary policy before routing, and its live-run mailbox additionally rejects all steering while the harness update marker is set. An existing run holds the shared execution lease; installation waits for the exclusive lease.

## Goals / Non-Goals

**Goals:** Preserve the native Worker notification wakeup path without weakening installation safety or ordinary steering policy.

**Non-Goals:** Change update fleet selection, manufacture Worker completion from idle/interrupted output, interrupt unrelated tools, or introduce a second notification protocol.

## Decisions

1. Recognize the existing notification envelope using both its reserved message-id prefix and prompt marker. Share that recognition between live steering and command routing. Checking prompt text alone would accidentally exempt ordinary messages; introducing a new wire command is unnecessary for this correction.
2. Route genuine notices to a steerable live mailbox before applying turn-boundary deferral. Preserve the fallback for non-steerable or absent runs, including the existing queued-message dispatch path.
3. Permit these notices through the pending-update marker only for the existing run. Keep mailbox acceptance and the routed-steer ledger together, and retain execution/update lease ownership. Clearing the update marker globally would allow unrelated new work to postpone installation and would weaken the gate.
4. Keep the existing OMP bridge's wait cancellation and result-before-notification ordering. Tests cover that seam without editing its protocol.

## Risks / Trade-offs

- A run may end while a notice is routed → retain the existing routed-steer reclaim/fallback path and durable command idempotency.
- A legacy id-only message resembles a notification → require both markers for the exemption; preserve existing frozen-queue fallback behavior.
- A pending update still waits for a run that continues doing work → unchanged exclusive lease policy; this correction removes the notification circular wait, not the update's intentional idle requirement.

## Migration Plan

Deploy with the next app build. No persisted schema migration. Reverting restores the old routing gate without changing saved commands or messages.
