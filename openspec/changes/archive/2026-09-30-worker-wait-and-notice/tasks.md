## 1. Wait status contract

- [x] 1.1 RED: controller MCP tests for unknown status rejection and for schema/description/help naming `completed` (`crates/workers-unpeel/tests/controller_mcp.rs`)
- [x] 1.2 Validate `status` in `wait_for_status` against `completed` plus reported lifecycle values; update schema description, tool description and `action=help`
- [x] 1.3 RED→GREEN: lifecycle wait returns on a completion transition during the wait; prior completion does not end it
- [x] 1.4 RED→GREEN: `completed` on a worker without task-episode tracking (no parent chat) is rejected immediately

## 2. Notification ends pending wait (OMP)

- [x] 2.1 RED: `omp_rpc` test with fake OMP and fake Workers controller: worker-notification steer during a long pending `wait_for_status`
- [x] 2.2 Recognise worker-notification steers and cancel the pending `wait_for_status`; deliver its interrupted result first, then the steer once
- [x] 2.3 Regression: ordinary steer keeps queue-until-result (`steer_during_pending_host_tool_is_consumed_once_after_tool_result`)
- [x] 2.4 RED→GREEN: a natural result racing the notification is preserved, steer once after it

## 3. Claude Worker composer

- [x] 3.1 Launch Claude Workers with prompt suggestions disabled; unit test on command/environment
- [x] 3.2 Output-tail test keeping a Claude permission dialog visible

## 4. Docs and verification

- [x] 4.1 DOX: `crates/workers-unpeel/AGENTS.md`, `crates/harness/AGENTS.md`, `third_party/unpeel/runtimes/README.md` if launch environment changes
- [x] 4.2 `cargo test -p zeron-workers-unpeel`; `cargo check -p zeron-harness --tests` (branch reviewed before merge; harness suite not re-run)

## 5. Review follow-ups (post-merge)

- [x] 5.1 `restart_worker` carries the parent chat binding to the replacement session (`carry_worker_parent_binding`); unit tests for carry, unbound source and already-bound target
- [x] 5.2 `worker_has_parent_binding` returns `Result`: an unreadable app state surfaces as a read error, not as "completion is not tracked"
- [x] 5.3 Lifecycle waits on an unbound worker skip the per-tick episode check
- [x] 5.4 DOX: `crates/workers-unpeel/AGENTS.md`
