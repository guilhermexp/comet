## 1. Removal
- [x] 1.1 Engine: OMP Chats go through the isolated title generator; drop native-title scheduling and run controls.
- [x] 1.2 Harness/proto: remove `generate_native_title`, `AgentEvent::NativeTitle` and the OMP `/rename` path with its tests and fixtures.
- [x] 1.3 Update owner AGENTS.md docs; build, focused tests, fmt.
- [x] 1.4 Validate and archive.

## Verification evidence
- Diagnosis (OMP 18.10.4, real adapter + `omp --mode rpc-ui` probes): `/rename` acks with `agentInvoked:false` at once and emits `session_info_update` ~28.6s later; the adapter read `sessionName` immediately, got none, and shut OMP down. Every Zeron OMP session file kept `"title": ""` and engine logs showed only prompt-fallback titles.
- `cargo build`: no warnings.
- `cargo test --no-fail-fast -p zeron-engine`: 988 passed; 2 load-sensitive failures (`rpc_dispatch_for_m5_methods` 4s RPC deadline, `a_message_with_attachments_holds_even_for_a_steerable_agent`) passed 3/3 in isolation.
- `cargo test --no-fail-fast -p zeron-harness -p zeron-doc -p zeron-proto`: 965 passed; 4 failures pre-exist on clean `origin/main` (3 skill-classification tests fail deterministically there; `stalled_prompt_post_has_a_bounded_timeout` fails 3/5 there).
- `cargo test -p zeron-engine --lib titles::`: passed, including mention-safe fallback.
