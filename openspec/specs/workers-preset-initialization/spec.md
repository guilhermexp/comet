# workers-preset-initialization Specification

## Purpose
Populate CLI Worker presets on first use while preserving the user's durable preset configuration across later refreshes.

## Requirements

### Requirement: Initialize fresh Worker presets once

Workers Settings SHALL populate a fresh uninitialized profile with the canonical built-in preset catalog, associating recognized commands with their runtime and current installation status. The resulting presets SHALL be persisted for the Worker launch catalog. Missing state and an uninitialized empty list SHALL both qualify as first use.

#### Scenario: First settings visit
- **WHEN** Settings reads a missing profile or an empty profile with no prior initialization markers
- **THEN** all canonical built-in presets appear with runtime association and installation status
- **AND** the next Worker bootstrap reads those persisted presets

Test: integration — `crates/workers-unpeel/tests/settings.rs`.

### Requirement: Preserve configured presets

Initialization SHALL be idempotent, preserve unrelated state, and SHALL NOT repopulate the full catalog for profiles with presets or prior initialization markers. Existing versioned catalog additions SHALL retain their current semantics. Unreadable or malformed preset data SHALL fail without being replaced.

#### Scenario: Delete all initialized presets
- **WHEN** the user deletes every preset after initialization and revisits Settings
- **THEN** the list stays empty

Test: integration — `crates/workers-unpeel/tests/settings.rs`.

#### Scenario: Existing configured profile
- **WHEN** a customized or native-migrated profile is read
- **THEN** its order, edits and deletions survive, except for the existing versioned additions
- **AND** unrelated state remains unchanged

Test: integration — `crates/workers-unpeel/tests/settings.rs`.

#### Scenario: Malformed presets
- **WHEN** persisted presets cannot be decoded
- **THEN** Settings reports an error and leaves the file unchanged

Test: integration — `crates/workers-unpeel/tests/settings.rs`.

### Requirement: Controller lists available enabled presets

The Worker controller SHALL advertise only presets both enabled and installed in the authoritative Settings projection, preserving Settings order, IDs, custom commands and existing scope. Favorites SHALL remain separate from order; fallback_order SHALL be one-based without gaps. Unavailable or disabled runtimes SHALL NOT be offered.

#### Scenario: Missing CLI still has an enabled toggle in persisted state
- **WHEN** Settings classifies an enabled preset as not installed
- **THEN** the controller omits it while returning installed enabled presets in their original relative order
- **AND** a favorite does not move ahead of an earlier non-favorite

Test: integration — `crates/workers-unpeel/tests/controller_mcp.rs`; unit — controller projection with deterministic runtime-catalog fixtures.

#### Scenario: Configured empty list or unavailable settings
- **WHEN** the initialized preset list is empty or Settings cannot read it
- **THEN** the controller returns an empty list or the explicit read error respectively
- **AND** it does not repopulate builtins, announce disabled CLIs or alter the persisted profile

Test: integration — `crates/workers-unpeel/tests/controller_mcp.rs` and `tests/settings.rs`.
