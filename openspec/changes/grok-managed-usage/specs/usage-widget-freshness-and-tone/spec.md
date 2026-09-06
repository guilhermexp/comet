# usage-widget-freshness-and-tone delta (grok-managed-usage)

## ADDED Requirements

### Requirement: Preserve last-known quota across transient probe failures

For slot-based providers probed by the engine (Claude, Codex), a failed or skipped forced usage probe SHALL preserve the account's last successfully probed quota windows instead of rendering the failure as empty usage. Last-known windows are device-local, in-memory, per account slot, and SHALL be invalidated when the account's credential changes.

#### Scenario: Transient probe failure keeps rendered quota

Test: engine unit test that probes successfully, fails the next forced probe, and asserts the previous windows are served.

- **WHEN** a forced usage probe fails after a successful probe for the same slot
- **THEN** the account snapshot keeps the slot's last-known windows
- **AND** the Usage widget rows remain in `Ready` state

#### Scenario: No prior success renders as before

Test: engine unit test with a failing first probe.

- **WHEN** a forced usage probe fails and the slot has never probed successfully
- **THEN** the account renders exactly as a probe failure did before this change
