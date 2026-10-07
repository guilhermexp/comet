## 1. Regression and implementation

- [x] 1.1 Add engine delivery tests and observe the notification routing failure before production changes.
- [x] 1.2 Restore direct Worker notification routing through turn-boundary and pending-update gates while retaining ordinary gates and lease safety.
- [x] 1.3 Run focused engine regressions and existing OMP wait-notification bridge tests.

## 2. Review and closeout

- [x] 2.1 Review the patch for races, duplicate delivery and gate regressions; address findings.
- [x] 2.2 Update engine DOX and test coverage, format and validate the change.
- [x] 2.3 Archive the validated OpenSpec change into the canonical capability.

## Validation evidence

- RED: pending-update and turn-boundary notification tests timed out before production changes; idle fallback passed. The unsteerable held-notification recovery test then reproduced removal of the held row before its guard was added.
- GREEN: `cargo test -p zeron-engine --test message_queue` — 36 passed, including all five new Worker cases.
- Ordinary update control: `cargo test -p zeron-engine --test e2e update_deferred_steering_preserves_the_active_turn_and_queued_prompt -- --exact` — 1 passed.
- OMP bridge: `cargo test -p zeron-harness --test omp_rpc notification` — 2 passed; `steer_during_pending_host_tool_is_consumed_once_after_tool_result` — 1 passed.
- All Cargo checks used one `scripts/cargo-verify.py` invocation with a disposable target; target cleanup completed.
- `cargo fmt --all -- --check` and `git diff --check` passed. Focused review found no material defects after the Worker-only unsteerable guard.
- Change and canonical `workers-host-bridge` spec passed strict validation. `openspec validate --specs` passed all 61 specs. Global strict validation flags two pre-existing Purpose placeholders in `global-file-preview` and `macos-dev-identity`, already present in HEAD.
- The running native app was not rebuilt, restarted or interrupted. The patch awaits the next app build; Update all fleet selection remains outside this change.
