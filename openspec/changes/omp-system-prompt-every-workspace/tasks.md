## 1. Implementation

- [x] 1.1 Remove the `~/.orchestrator` workspace gate from `OmpHarness::run`; always append `orchestrator_system_prompt(workers_expected)`.
- [x] 1.2 Remove `orchestrator_workspace`, `with_orchestrator_workspace` and `is_orchestrator_workspace`.

## 2. Tests

- [x] 2.1 `every_omp_chat_run_receives_the_omp_system_prompt` (project cwd, fixture `require-system-prompt`) replaces the two workspace tests; the `reject-system-prompt` fixture scenario is removed.
- [x] 2.2 `cargo test -p zeron-harness --test omp_rpc` green.

## 3. Docs

- [x] 3.1 `crates/harness/AGENTS.md`: system prompt gates.
