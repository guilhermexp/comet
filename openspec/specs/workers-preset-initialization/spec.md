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
