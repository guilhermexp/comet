# Change: Clean macOS build diagnostics

## Why

A successful macOS build emits hundreds of legacy Objective-C macro warnings, missing bundled-font warnings at runtime, and future-incompatibility notices from two transitive crates. The noise hides actionable diagnostics and includes dependencies that a future Rust compiler will reject.

## What Changes

- Declare only the legacy `cargo-clippy` cfg expected by Objective-C macros while preserving `unexpected_cfgs` checking for every other value.
- Serve GPUI's expected SVG font asset paths from Comet's existing embedded font assets.
- Remove the future-incompatible transitive code through minimal local dependency compatibility fixes without changing the pinned GPUI upstream revision.
- Add gates that fail if these known diagnostics return.

## Capabilities

### New Capabilities

- `macos-build-diagnostics`: clean, actionable macOS build and SVG-renderer diagnostics on the supported toolchain.

### Modified Capabilities

None.

## Impact

- Workspace/UI Cargo lint and dependency configuration.
- `crates/ui` embedded asset routing and tests.
- Minimal local compatibility patches for `block` and `proc-macro-error2` only if dependency resolution cannot select fixed releases.
- No updater, app version, release workflow, or upstream GPUI revision change.
