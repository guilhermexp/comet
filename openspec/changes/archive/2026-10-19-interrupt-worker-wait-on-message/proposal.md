# Proposal

## Why

A user message sent while the Orchestrator is inside `workers.wait_for_status` remains unconsumed until the wait finishes. The prior OMP fix deliberately interrupts only Worker notifications; native Claude sends ordinary messages with `priority: next` whenever a tool is open, including this read-only wait.

## What Changes

- An incoming message releases the Orchestrator's pending Workers status wait promptly instead of waiting for its timeout.
- Preserve the Worker process/task, unrelated tool execution, natural results that win a race, and exactly-once delivery of the incoming message.
- Apply the behavior at the common controller/engine boundary used by native Claude, Pi, Codex and ACP, and generalize OMP's existing request-scoped cancellation; do not shorten timeouts or force provider-wide interruption.
- Replace the canonical ordinary-message queue-until-wait-result scenario with the interruptible-wait contract; ordinary non-wait tools retain their existing protection.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `workers-host-bridge`: incoming-message interruption of the parent's pending wait, message ordering and preservation of Workers/unrelated tools.

## Impact

The engine's existing accepted-message ledger, one read-only local RPC, the Workers controller, OMP's bridge, focused integration fixtures/tests, nearest DOX, and the existing Workers host bridge specification. No persisted schema, Worker lifecycle hook, provider account configuration, release or packaging change is intended.
