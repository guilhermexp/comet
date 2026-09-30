## 1. Wait status contract

- [ ] 1.1 RED: controller MCP tests for unknown status rejection and for schema/description/help naming `completed` (`crates/workers-unpeel/tests/controller_mcp.rs`)
- [ ] 1.2 Validate `status` in `wait_for_status` against `completed` plus reported lifecycle values; update schema description, tool description and `action=help`
- [ ] 1.3 RED→GREEN: lifecycle wait returns on a completion transition during the wait; prior completion does not end it
- [ ] 1.4 RED→GREEN: `completed` on a worker without task-episode tracking (no parent chat) is rejected immediately

## 2. Notification ends pending wait (OMP)

- [ ] 2.1 RED: `omp_rpc` test with fake OMP and fake Workers controller: worker-notification steer during a long pending `wait_for_status`
- [ ] 2.2 Recognise worker-notification steers and cancel the pending `wait_for_status`; deliver its interrupted result first, then the steer once
- [ ] 2.3 Regression: ordinary steer keeps queue-until-result
- [ ] 2.4 RED→GREEN: a natural result racing the notification is preserved, steer once after it

## 3. Claude Worker composer

- [ ] 3.1 Launch Claude Workers with prompt suggestions disabled; unit test on command/environment
- [ ] 3.2 Output-tail test keeping a Claude permission dialog visible

## 4. Docs and verification

- [ ] 4.1 DOX: `crates/workers-unpeel/AGENTS.md`, `crates/harness/AGENTS.md`, `third_party/unpeel/runtimes/README.md` if launch environment changes
- [ ] 4.2 `scripts/cargo-verify.py -- cargo test -p zeron-workers-unpeel -p zeron-harness -p unpeel-core`; `cargo build`
