# macos-build-diagnostics Specification

## Purpose
Keep supported macOS builds and native SVG rendering free of known compatibility noise so new warnings remain visible and future Rust upgrades remain possible.

## Requirements

### Requirement: Objective-C compatibility cfg is narrowly declared

The macOS UI build SHALL accept the legacy `cargo-clippy` feature cfg emitted by Objective-C macros while retaining unexpected-cfg diagnostics for undeclared values.

#### Scenario: UI crate is checked on macOS

Test: `cargo check -p zeron-ui --message-format short` with diagnostic assertion (`integration`).

- **WHEN** the UI crate expands Objective-C macros
- **THEN** no `unexpected cfg condition value: cargo-clippy` warning is emitted
- **AND** the unexpected-cfg lint remains enabled

### Requirement: SVG fallback fonts are embedded

The packaged native app SHALL resolve the sans-serif and monospace font asset paths requested by the SVG renderer without filesystem or network access.

#### Scenario: SVG renderer requests fallback fonts

Test: `Assets` load/list unit regression in `crates/ui/src/icons.rs` (`unit`).

- **WHEN** the renderer requests either supported fallback font path
- **THEN** the asset source returns embedded font bytes
- **AND** no `Bundled font not found` warning is emitted during native SVG rendering

### Requirement: Supported toolchain has no known future-incompatible dependency code

The workspace SHALL build on the supported Rust toolchain without future-incompatibility reports for `block 0.1.6` or `proc-macro-error2 2.0.1`.

#### Scenario: Future incompatibility report is generated

Test: `cargo check -p zeron && cargo report future-incompatibilities` (`integration`).

- **WHEN** the workspace is checked with the supported Rust toolchain
- **THEN** neither targeted package appears with reject-in-future code
- **AND** the pinned GPUI upstream revision and app version remain unchanged
