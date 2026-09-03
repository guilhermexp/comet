# Design: UI Dev Inspector

## Context

Comet uses a pinned rev of the `wingleeio/zed` GPUI fork (`Cargo.toml`). GPUI already compiles the inspector mechanism in debug builds (`#[cfg(any(feature = "inspector", debug_assertions))]`), including `Window::toggle_inspector`, hit-testing, mouse event interception during picking, and element highlight rendering. However, without app-side wiring and an `InspectorRenderer` registered via `App::set_inspector_renderer`, `Inspector::render` renders `Empty` and no picking mode can be activated.

## Goals

- Provide a dev-only toggleable inspector panel via shortcut (`cmd-alt-i`) and menu item.
- Enable element picking mode (`inspector.start_picking()`) where hovering highlights elements and clicking selects an element.
- Display the selected element's `source_location` (`file:line`) and `instance_id` in selectable, copiable form.
- Ensure the inspector is completely excluded in release builds (`#[cfg(debug_assertions)]`), with `cargo check --release -p zeron` passing.
- Keep the implementation clean and self-contained within `crates/ui` without any GPL code or external dependencies.

## Non-goals

- Live style editing or CSS modification.
- Click-to-open in external editors via CLI shell-out.
- Enabling the `inspector` feature on GPUI or altering Cargo profile configurations.
- Any change to the release build or end-user configurations.

## Decisions

### 1. Action registration and window dispatch
Register `actions!(dev, [ToggleInspector])` in `crates/ui/src/inspector.rs`.
In `inspector::init(cx)`, register the global action handler using `cx.defer` before calling `active_window.update(cx, |_, window, cx| window.toggle_inspector(cx))`. This deferral is mandatory to avoid double-leasing the window during action dispatch.

### 2. Inspector panel renderer
Install the inspector renderer via `cx.set_inspector_renderer(Box::new(render_inspector))`.
The renderer renders a right-docked panel containing:
- A header with title "GPUI Inspector", a "DEV" badge, and a shortcut hint.
- A "Pick Element" / "Picking element…" button that toggles `inspector.start_picking()` and calls `window.refresh()`.
- Active element information when an element is selected:
  - Source location path and line number (`id.path.source_location.file()` and `id.path.source_location.line()`).
  - Instance disambiguation ID (`id.instance_id`).
  - Copy location action for developer convenience.
- Fallback instructions when no element has been picked yet.
- Container rendering child states from `inspector.render_inspector_states(window, cx)`.

### 3. Keymap integration
In `crates/ui/src/shell.rs`, inside `apply_keymap`, register `KeyBinding::new(&platform_combo("mod-alt-i"), crate::inspector::ToggleInspector, None)` under `#[cfg(debug_assertions)]`. This ensures that even though `apply_keymap` clears all key bindings at boot and on config reload, the inspector shortcut remains bound.

### 4. Zero release impact
All code and references are strictly guarded by `#[cfg(debug_assertions)]`. In release builds, `inspector.rs` is not compiled, no actions or keybindings are registered, and binary size/performance remain untouched.

## Risks

- **Double window lease:** Calling `window.toggle_inspector` synchronously from an action handler can cause a runtime panic. Mitigated by `cx.defer(...)`.
- **Keymap clearing on boot:** `shell::apply_keymap` clears the keymap during boot. Mitigated by registering the binding inside `apply_keymap` directly.
- **Release compilation failure:** If any inspector API is called without `#[cfg(debug_assertions)]`, `--release` builds will fail. Mitigated by comprehensive `#[cfg(debug_assertions)]` gating and testing with `cargo check --release -p zeron`.
