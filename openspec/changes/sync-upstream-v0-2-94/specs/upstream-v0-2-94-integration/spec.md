## ADDED Requirements

### Requirement: Upstream v0.2.94 behavior is available without regressing fork contracts

The fork SHALL include upstream behavior through `433aa148` while keeping OMP, Workers, Run/Steer steering, the fork MCP grant model, accounts/usage presentation, fork visual design and fork release policy intact.

#### Scenario: Workspace builds and tests after the merge

Test: `cargo check --workspace --all-targets` and `cargo test --workspace`.

- **WHEN** the sync branch is built
- **THEN** every crate, test and example compiles
- **AND** the workspace test suite passes, with pre-existing network/credential-only cases still ignored

#### Scenario: Runs keep the fork MCP grant

Test: `crates/harness` `workers_mcp` tests (`root_grant_also_carries_the_zeron_chat_mcp`, `workers_only_omits_sessions`).

- **WHEN** a non-orchestrator chat starts a run
- **THEN** no Zeron/sessions MCP server is injected into its harness arguments
- **AND WHEN** a root orchestrator run holds the sessions grant
- **THEN** it receives both `comet-sessions` and the `zeron` chat MCP stamped with its own chat id

### Requirement: Comet never updates from the upstream Zeron feed

The updater SHALL NOT treat the upstream Zeron edge release feed as a Comet feed.

#### Scenario: Default edge without an explicit feed

Test: `zeron-update` unit `upstream_zeron_edge_is_not_a_comet_release_feed`.

- **WHEN** the edge is `edge.zeron.sh` and `ZERON_RELEASES_URL` is unset
- **THEN** no release base resolves and the engine does not start the release checker
