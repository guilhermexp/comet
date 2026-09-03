## 1. Objective-C cfg diagnostics

- [x] 1.1 Add a diagnostic gate that reproduces the legacy `cargo-clippy` cfg warning.
- [x] 1.2 Declare only that expected cfg value for `zeron-ui` and prove other unexpected cfg values remain linted.

## 2. SVG font assets

- [x] 2.1 Replace the existing empty-font assertion with failing load/list tests for GPUI's two requested paths.
- [x] 2.2 Route those paths to existing embedded Geist sans/mono bytes and verify native SVG rendering emits no missing-font warning.

## 3. Future-compatible dependencies

- [x] 3.1 Capture the exact `block 0.1.6` and `proc-macro-error2 2.0.1` future-incompatibility reports and dependency chains.
- [x] 3.2 Select compatible fixed releases, or add minimal licensed local patches under `third_party` when resolution cannot do so.
- [x] 3.3 Prove the current build emits no future-incompatibility warning or new report and no pinned GPUI/app version changed. Historical Cargo report IDs remain historical records.

## 4. Closeout

- [x] 4.1 Update affected root, UI, and third-party DOX docs and verification matrices.
- [x] 4.2 Run formatting, focused diagnostics, workspace tests, app build, and a bounded native macOS smoke.
