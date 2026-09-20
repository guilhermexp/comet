# macos-dev-identity Specification

## Purpose
TBD - created by archiving change fix-macos-cargo-app-identity. Update Purpose after archive.

## Requirements

### Requirement: Native development identity
On macOS, headed `cargo run` SHALL execute the selected build from a local Zeron.app using the distribution manifest and icon, without intercepting termination signals. Generated files SHALL remain beside that build artifact.

#### Scenario: Headed launch
- **WHEN** Cargo runs zeron with no arguments or a single zeron:// URL
- **THEN** the executable runs from Zeron.app/Contents/MacOS/zeron with its original cwd, environment, arguments, stdio and exit status
- **AND** the native window is identified as Zeron and survives cmux duplicate-instance detection
- Test: integration — scripts/test-cargo-macos-runner.py; native cmux validation

#### Scenario: CLI and test execution
- **WHEN** Cargo runs a zeron CLI subcommand or a different executable
- **THEN** the original executable and arguments run directly
- Test: integration — scripts/test-cargo-macos-runner.py

#### Scenario: Rebuild and shutdown
- **WHEN** the binary is rebuilt or receives SIGTERM
- **THEN** the next launch uses the rebuilt binary and normal signal termination is preserved
- Test: integration — scripts/test-cargo-macos-runner.py
