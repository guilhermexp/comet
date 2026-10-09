# Tasks

- [x] 1. Reproduce blocked control output, uncooperative observer and cancelled-stop ownership and serial execution teardown with focused failing tests.
- [x] 2. Implement the bounded cleanup and update the owning DOX contract.
- [x] 3. Verify new and existing Live Voice and execution shutdown tests, obtain independent review, format, validate and archive the change.

## Verification evidence

- RED: three initial Voice regressions failed for blocked control output, an unresponsive observer and cancelled-waiter ownership; the stopping-reservation race also failed.
- RED: independent run teardown exceeded its 5-second test deadline in the serial implementation.
- GREEN: all 20 Live Voice tests passed; concurrent shutdown passed in 3.07 seconds; the existing dispatch-admission regression passed.
- Independent review approved after synchronized admission/state transitions and cancellation Drop ordering were corrected. Scoped formatting and diff checks passed.
- This bounds cooperative asynchronous cleanup; synchronously blocked work cannot be forcibly interrupted by Tokio. It does not claim historical quit logs identify one exclusive cause.
- Graph refresh was attempted; `graft` is unavailable in this environment. Existing cards were consulted and their shifted spans checked against current symbols.
