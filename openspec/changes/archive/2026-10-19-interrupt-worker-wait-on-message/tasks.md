# Tasks

## 1. Incoming messages during Workers waits

- [x] 1.1 Add engine contract coverage and focused OMP/controller regressions; observe OMP/controller behavioral failures before production changes and retain natural-result/unrelated-tool controls.
- [x] 1.2 Expose the existing pending-steer ledger through a read-only local `WorkerWaitState` RPC; verify acceptance, consumption, queued-only state and Chat isolation through the engine.
- [x] 1.3 Make the parent-bound controller return its status wait when that RPC reports a pending message, reusing bounded transport; verify early normal result, natural-match precedence and no-message control without mutating Workers.
- [x] 1.4 Generalize OMP request-scoped wait cancellation to incoming messages, preserving one result before one steer; verify `omp_rpc` wait, cancellation, duplicate-id and ordinary-tool regressions.
- [x] 1.5 Update engine/RPC/controller/harness contracts and the canonical Workers wait contract with the common behavior and exercised runtime limits.

## 2. Integrated proof

- [x] 2.1 Run focused harness tests under `scripts/cargo-verify.py`, formatting checks, and a disposable runtime smoke of the changed wait/message path without restarting the user's app or touching provider configuration; record actual evidence and any visual limits.
- [x] 2.2 Validate the change and synchronize its delta into the canonical spec, preserving unrelated local WIP.

## Workflow follow-up

- Archive the completed change after verification. No commit, push or release is included in this request.
