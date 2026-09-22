## ADDED Requirements

### Requirement: Upstream v0.2.83 behavior is available without regressing fork contracts

The fork SHALL include upstream behavior through `v0.2.83` while keeping OMP, Workers, Live Voice, Run/Steer steering, trajectory, accounts/usage, fork visual design and fork release policy intact.

#### Scenario: Workspace builds and tests after the merge

Test: `cargo check --workspace --all-targets` and `cargo test --workspace`.

- **WHEN** the sync branch is built
- **THEN** every crate, test and example compiles
- **AND** the workspace test suite passes, with pre-existing network/credential-only cases still ignored

#### Scenario: Message sent during a live run

Test: composer and doc command unit tests (`newer_steer_preserves_older_pending_steer`, composer steering tests).

- **WHEN** the user presses Enter while a run is active on a steering-capable harness
- **THEN** the message is delivered as a steer, not silently queued
- **AND** every batched remote steer executes exactly once

#### Scenario: Upstream RPC surface is reachable on remote devices

Test: `crates/rpc` registry tests plus engine device-routing tests.

- **WHEN** a client calls an upstream-added method (install, queue, project action, workspace image/file, git status) with `targetDeviceId`
- **THEN** the relay forwards it with upstream's stream and deadline rules

### Requirement: Fork publication boundary is preserved

The sync SHALL NOT add automatic publication or scheduled automation to the fork.

#### Scenario: CI workflows after the merge

Test: inspection of `.github/workflows`.

- **WHEN** the merge is committed
- **THEN** `release.yml` matches the fork version and no scheduled upstream PR-opening workflow is present
- **AND** the workspace version remains the fork's `0.2.18`
