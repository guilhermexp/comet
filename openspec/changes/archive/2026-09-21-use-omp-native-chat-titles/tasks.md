## 1. Integration
- [x] 1.1 Prove installed CLI native invocation without an extra coding turn or Comet model selection.
- [x] 1.2 Test and implement native OMP title adapter and completed-session scheduling.
- [x] 1.3 Verify user-title precedence, failure behavior and existing title paths.
- [x] 1.4 Review, update DOX, validate and archive.

## Verification evidence
- Installed OMP 18.2.7: `/rename` without arguments returned `agentInvoked:false` and a native title on a temporary session copy. Probe cleaned up; original session untouched.
- `cargo test -p zeron-harness -- --test-threads=4`: passed, including all 62 non-live OMP RPC tests and the other harness adapters.
- `cargo test -p zeron-engine --lib titles::`: 10 passed.
- `cargo build`: passed.
- `cargo fmt --all` and `git diff --check`: passed.
- Review found unnecessary native generation for named Chats/explicit overrides; fixed with host-local opt-in in RunControls.
- One unconstrained parallel OMP suite run hit the existing handshake timing assertion in `a_dying_child_reports_its_own_stderr_not_a_bare_transport_error`; the full suite passed with four test threads.
- `cargo test -p zeron-engine --test e2e native_title -- --nocapture`: 2 passed (native metadata stays local; opt-in only for untitled Chats without override).
- Final review confirmed the opt-in fix; no new concrete issue found.
