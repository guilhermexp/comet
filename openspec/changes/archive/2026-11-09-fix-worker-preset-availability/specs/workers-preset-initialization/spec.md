## ADDED Requirements

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
