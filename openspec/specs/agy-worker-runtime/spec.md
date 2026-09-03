# agy-worker-runtime Specification

## Purpose

Antigravity CLI (agy) runtime package declaration, controller MCP readiness probing, UI presentation, and default preset migration in Unpeel.

## Requirements

### Requirement: Declare Antigravity CLI (agy) runtime package in Unpeel

The system SHALL declare `agy` as a built-in agent runtime package with reverse-DNS identifier `com.google.antigravity-cli`, `legacy_order = 15`, `lifecycle.source = "output"`, `tint = "#4285F4"`, `spinner_tint = "#4285F4"`, `command_aliases = ["agy"]`, and suggested preset `agy --dangerously-skip-permissions`.

#### Scenario: Runtime descriptor validates and registers in catalog
Test: `cd third_party/unpeel && bun run validate:runtimes`

- **WHEN** the unpeel runtime catalog is validated and compiled
- **THEN** `com.google.antigravity-cli` is discovered with exact capabilities `["resume", "restart_agent"]`
- **AND** client metadata generators produce matching Swift, TS, and Rust catalog snapshots without drift

### Requirement: Resume and restart Antigravity CLI sessions

The system SHALL support resuming Antigravity CLI sessions via `--conversation <id>` for explicit conversation IDs and `--continue` / `-c` for continue-last sessions, while preserving other arguments and the original command structure.

#### Scenario: Resume with conversation ID
Test: unpeel-core unit test for agy resume adapter

- **WHEN** a session is resumed with provider conversation ID `a5b41b44`
- **THEN** the resulting command is `agy --dangerously-skip-permissions --conversation 'a5b41b44'`

#### Scenario: Resume without explicit ID
Test: unpeel-core unit test for agy resume adapter

- **WHEN** a session is resumed without an explicit ID and no prior resume marker
- **THEN** `--continue` is appended to the command

### Requirement: Automatically trust launched workspace for Antigravity CLI

The system SHALL idempotently add the session launch workspace path into `trustedWorkspaces` in `~/.gemini/antigravity-cli/settings.json` upon session launch, preserving any existing keys such as `model` and `permissions` and writing atomically.

#### Scenario: Launch in an untrusted workspace
Test: unpeel-core unit test for agy setup adapter

- **WHEN** a worker launches in a directory `/path/to/project`
- **THEN** `/path/to/project` is added to `trustedWorkspaces` in `settings.json`
- **AND** subsequent interactive launch does not halt at the trust confirmation prompt

### Requirement: Detect readiness in Controller MCP

The Controller MCP SHALL recognize Antigravity CLI prompt readiness when the screen contains both `antigravity cli` and `for shortcuts`, and SHALL NOT mark readiness when showing the blocking trust selection prompt.

#### Scenario: Interactive prompt ready
Test: integration test in `crates/workers-unpeel/tests/controller_mcp.rs`

- **WHEN** the terminal screen shows the Antigravity header and `? for shortcuts`
- **THEN** `is_briefing_screen_ready` evaluates to `true`

#### Scenario: Trust selector prompt
Test: integration test in `crates/workers-unpeel/tests/controller_mcp.rs`

- **WHEN** the terminal screen shows `Do you trust the contents of this project?`
- **THEN** `is_briefing_screen_ready` evaluates to `false`

### Requirement: Seed agy preset in existing profiles via v2 migration

The system SHALL seed the `agy` preset for existing user state by migrating `comet_workers_preset_catalog_version` from 1 to 2, adding `agy` if absent without resurrecting previously deleted presets.

#### Scenario: Migrating profile from v1 to v2
Test: integration test in `crates/workers-unpeel/tests/settings.rs`

- **WHEN** an existing profile with version 1 is loaded
- **THEN** the `agy` preset is added to the presets list
- **AND** `comet_workers_preset_catalog_version` becomes 2
- **AND** any previously deleted preset (e.g. `omp`) remains deleted
