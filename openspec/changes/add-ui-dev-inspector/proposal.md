# Change: Add UI Dev Inspector

## Why

During UI development in Comet, developers need to quickly inspect rendered GPUI elements to identify their source code origin (`file:line` and instance ID) without manual grepping or guessing. The underlying GPUI fork already includes the inspector mechanism under `debug_assertions`, but lacks app-side wiring and an inspector renderer in Comet.

## Decisions

- **D-01:** Gate all inspector code under `#[cfg(debug_assertions)]` so release builds are completely unaffected and compile with zero inspector overhead.
- **D-02:** Register `ToggleInspector` action in `crates/ui/src/inspector.rs` with `cx.defer` around window toggle to avoid double lease issues.
- **D-03:** Install an inspector renderer via `App::set_inspector_renderer` built exclusively with GPUI primitives and Comet's native UI theme, icons, and typography.
- **D-04:** Bind `cmd-alt-i` in `shell::apply_keymap` so the shortcut survives keymap re-application after boot.
- **D-05:** Do not enable the `inspector` Cargo feature on GPUI or modify profile flags, preserving existing build configurations and licensing boundaries (no Zed GPL crates).

## What Changes

- Add new module `crates/ui/src/inspector.rs` under `#[cfg(debug_assertions)]` with action definition, initialization, and inspector panel renderer.
- The inspector panel provides an interactive picking button (`inspector.start_picking()`), displays the active element's `source_location` and `instance_id` with copy affordance, and renders registered inspector element states.
- Call `inspector::init(cx)` at application boot in `crates/ui/src/lib.rs` under `#[cfg(debug_assertions)]`.
- Bind `cmd-alt-i` to `ToggleInspector` in `crates/ui/src/shell.rs` inside `apply_keymap` under `#[cfg(debug_assertions)]`.
- Add an optional "Toggle GPUI Inspector" item in the Developer / View menu under `#[cfg(debug_assertions)]`.
- Update `crates/ui/AGENTS.md` (append-only) to document the dev inspector capability and shortcut.

## Capabilities

### New Capabilities

- `ui-dev-inspector`: Native GPUI inspector for inspecting UI element source locations in development builds.

## Impact

- `crates/ui`: Module `inspector.rs`, boot hook in `lib.rs`, keymap in `shell.rs`, menu in `app_menus.rs`, and DOX in `AGENTS.md`.
- No impact on `proto`, `doc`, `sync`, `harness`, `engine`, `rpc`, or release builds.
