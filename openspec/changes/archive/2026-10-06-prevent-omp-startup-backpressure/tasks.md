# Tasks

- [x] 1. Reproduce negotiation deadlock with a startup event burst larger than the live channel.
- [x] 2. Implement bounded ordered startup buffering and update harness DOX.
- [x] 3. Verify focused OMP transport regressions, independent review, formatting and strict OpenSpec validation; archive and document limits of historical attribution.

## Verification evidence

- RED: 300 startup events blocked the negotiation/follow-up response and caused an RPC timeout. A temporary global nonblocking policy failed the paused-live-consumer test; the final implementation confines it to startup.
- GREEN: 69 OMP RPC fixture tests passed, with two real-runtime tests explicitly ignored by the fixture suite. The installed-runtime catalog test was then run explicitly and passed in 2.96 seconds without a model turn. The local command burst regression was expanded to 1,100 events and passed in its focused rerun.
- Startup order, 1,024-event overflow, live backpressure, frame/chunk size rejection, cancellation and child cleanup are covered. Independent review approved; formatting, fixture shell syntax and diff checks passed.
- Current native startup probes with extensions enabled produced ready in about 250 ms. This does not explain historical 15-second zero-stdout failures. Timeout logs now include the resolved executable and cwd for a recurrence.
- Graph refresh was attempted; the graft CLI is unavailable. Existing cards were consulted and shifted source spans checked by current symbols.
