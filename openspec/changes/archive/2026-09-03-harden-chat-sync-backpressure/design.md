## Context

See `proposal.md` for motivation. The client already tracks a five-second quota retry deadline, but enqueue nudges call the bulk-send path without consulting that state. The registry integration test imports a fixture module that is exposed only by the `mock-server` feature.

## Goals / Non-Goals

**Goals:** make quota state the single gate for all eager sends; preserve queue order; make the documented sync gate explicit and reproducible.

**Non-Goals:** alter edge quotas, protocol frames, checkpoint behavior, or ordinary non-quota retry policy.

## Decisions

- **D1:** Guard the enqueue nudge with the existing quota state and deadline. This extends current state instead of introducing a second rate limiter.
- **D2:** On deadline, use the existing head-only retry path. Bulk replay was rejected because it recreates the server burst.
- **D3:** Make `--features mock-server` the explicit local sync integration gate and record it in the owning DOX verification matrix. Exposing the mock server in production/default builds was rejected.

## Risks / Trade-offs

- [A missed send entry point bypasses cooldown] → enumerate every `push_pending`/`push_head` caller and cover enqueue plus timer paths.
- [An unacknowledged head stalls following edits] → retain the existing retry/ack state machine and virtual-clock coverage.

## Migration Plan

No data or protocol migration. Roll back the client guard if the focused tests reveal a compatibility regression.
