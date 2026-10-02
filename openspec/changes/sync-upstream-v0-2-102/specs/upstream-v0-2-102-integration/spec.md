## ADDED Requirements

### Requirement: Upstream v0.2.102 behavior is available without regressing fork contracts

The fork SHALL include upstream behavior through `64ad6f6e` while keeping OMP and Kimi, Workers, Run/Steer steering, Live Voice, the fork MCP grant model, accounts/usage presentation, fork visual design and fork release policy intact.

#### Scenario: Workspace builds and tests after the merge

Test: `cargo check --workspace --all-targets` and `cargo test --workspace`.

- **WHEN** the sync branch is built
- **THEN** every crate, test and example compiles
- **AND** the workspace test suite passes, with pre-existing network/credential-only cases still ignored

#### Scenario: Agent CLI updates keep the fork package-manager plan

Test: `zeron-engine` `harness_updates` unit tests `codex_updates_through_the_package_manager_that_owns_the_install` and `omp_is_tracked_and_updates_itself`.

- **WHEN** a Codex install is owned by npm or Homebrew
- **THEN** the coordinator updates it through that package manager, never resolved from PATH
- **AND** OMP stays monitored and updates itself
