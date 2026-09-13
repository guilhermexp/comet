## Why

Workers Settings reads an empty preset list on fresh profiles even when CLIs are installed. The adapter migrates existing presets but never initializes the built-in catalog that the original native frontend supplied.

## What Changes

- Initialize the built-in presets once when Settings reads a fresh, uninitialized profile.
- Persist the catalog so subsequent Worker launch catalogs see the same presets.
- Preserve customized profiles, intentional deletions, and existing versioned migrations.

## Capabilities

### New Capabilities
- `workers-preset-initialization`: First-use preset population and durable preservation of user edits.

### Modified Capabilities

None.

## Impact

`crates/workers-unpeel/src/lib.rs`, its settings integration tests, and domain documentation. No CLI installation, dependency, or wire changes.
