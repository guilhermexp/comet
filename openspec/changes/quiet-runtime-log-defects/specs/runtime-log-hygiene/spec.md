## ADDED Requirements

### Requirement: Verified chat cursors are not repaired again

Once a chat's snapshot has been written with a verified cursor, later client admissions on the same open chat SHALL NOT repeat the legacy cursor repair.

#### Scenario: A born chat is re-admitted after its first verified write

Test: `first_verified_write_certifies_the_warm_handle` in `crates/engine/src/chat_persistence.rs` (`unit`).

- **WHEN** a chat whose stored snapshot is unverified applies rows and flushes a verified snapshot
- **THEN** its persistence reports a verified cursor
- **AND** a re-admitted client keeps that cursor instead of clamping it to the checkpoint

### Requirement: Usage probes back off exponentially on transient failures

Managed Provider Usage probes SHALL lengthen the wait after each consecutive transient failure (rate limit, server error, network) up to a cap, honor a longer server Retry-After, and reset on success.

#### Scenario: A provider answers 429 with Retry-After 0 repeatedly

Test: `repeated_rate_limits_space_out_probes_until_a_success` and `transient_failures_escalate_and_credential_failures_do_not` in `crates/engine/src/agent_accounts.rs` (`unit`).

- **WHEN** consecutive probes are rate limited with `Retry-After: 0`
- **THEN** the scheduled waits grow 30 s, 60 s, 120 s, … up to 30 minutes
- **AND** a success clears the failure count and the retry time
- **AND** credential-bound failures keep their fixed wait

### Requirement: An offline relay verdict parks peer dials

When the device relay reports `host_offline`, the peer link cache SHALL stop dialing that device for the offline cooldown regardless of network-recovery broadcasts or token refreshes, until fresh presence says the device is alive or the user signs out.

#### Scenario: A network-recovery broadcast arrives while a peer is offline

Test: `host_offline_cooldown_survives_online_broadcasts` in `crates/rpc/tests/device_room.rs` (`integration`).

- **WHEN** a dial fails with the relay's `host_offline` verdict and the process then broadcasts that the network is online
- **THEN** the next call fails fast as offline without dialing
- **AND** a presence-driven cooldown reset lets the next call dial immediately

### Requirement: Repeated identical failures warn once per episode

Background loops covered by this capability (usage probes, chat push quota) SHALL emit a warning on the first failure or a change of failure class, and lower-severity diagnostics while the same failure repeats.

#### Scenario: A streaming chat stays over the push quota

Test: `none` — logging level only; behavior covered by existing chat client quota tests.

- **WHEN** the server rejects a burst of pushes and later head probes with `quota`
- **THEN** one warning is logged for the episode
- **AND** the remaining rejections log at debug

### Requirement: Terminal-session shims do not outrank stable installs

Harness executable resolution SHALL prefer a stable install over an equally versioned per-terminal-session shim directory, while still resolving a lone shim.

#### Scenario: A cmux shim precedes the real codex on PATH

Test: `terminal_session_shims_do_not_outrank_a_stable_install` in `crates/harness/src/executable.rs` (`unit`).

- **WHEN** a `cmux-cli-shims` candidate and a stable candidate report the same version
- **THEN** the stable candidate is selected
- **AND** a lone shim candidate is still selected

### Requirement: Preview discovery ignores other Zeron engines

The preview scanner SHALL NOT HTTP-probe listeners whose program is a Zeron engine.

#### Scenario: Another Zeron engine listens inside an open project

Test: `other_zeron_engines_are_not_preview_candidates` in `crates/preview/src/service.rs` (`unit`).

- **WHEN** a listener's program is the `zeron` executable
- **THEN** it is classified as a Zeron engine and excluded from probing

### Requirement: App-owned quit drains the in-process engine first

An app-owned quit SHALL hide the app and complete (or time out) the in-process engine shutdown before asking GPUI to terminate.

#### Scenario: The user quits with an in-process engine

Test: headed `cargo run` quit smoke — no `timed out waiting on app_will_quit` in the log (`e2e`).

- **WHEN** the user quits through the menu, ⌘Q, or the Dock
- **THEN** the windows hide and the engine drain runs under a bounded budget
- **AND** GPUI's quit runs afterwards without timing out on quit handlers
