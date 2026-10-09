# Tasks

## 1. Registry lifecycle

- [x] 1.1 Reproduce registry dial/handshake/backpressure shutdown, cancelled shutdown ownership and active-session wake with focused failing actor tests; implement cancellation and verify those tests plus existing registry tests pass.
- [x] 1.2 Update sync DOX lifecycle/coverage documentation and verify it matches the tested registry behavior.

## 2. Chat and device-room recovery

- [x] 2.1 Reproduce active Chat/device-host wake recovery and offline-cooldown preservation with focused failing transport tests; implement prompt redial and preserve outbox/catch-up semantics; verify focused and existing transport tests pass.
- [x] 2.2 Update owning sync/RPC DOX and verify coverage commands and remaining network limitations.

## 3. Integrated closeout

- [x] 3.1 Complete independent review, formatting, diff checks and strict OpenSpec validation; archive only after all required tests pass and record unresolved external DNS/provider failures separately.

## Verification evidence

- RED: five initial registry regressions, two Chat wake regressions and two device-room wake/cooldown regressions failed on the original behavior.
- GREEN: 75 sync unit tests, 11 registry integration tests, 22 RPC unit tests and 15 device-room integration tests passed; the live-edge test remains explicitly ignored because it requires a running edge.
- Added pending-dial cancellation and pending-batch identity/ACK coverage after independent review. Review found no remaining material issues; host token/dial and Chat pre-ready handshake have structural coverage but no separate scenario test.
- Scoped formatting and diff checks passed. Historical DNS/provider failures are external observations; the changes repair local transport lifecycle and do not guarantee network availability.
