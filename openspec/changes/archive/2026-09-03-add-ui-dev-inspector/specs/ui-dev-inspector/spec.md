## ADDED Requirements

### Requirement: Toggle GPUI Inspector in debug builds

The system SHALL provide a dev-only action `ToggleInspector` in debug builds that toggles the native GPUI inspector panel on the active window using a deferred dispatch to avoid double-leasing the window. The inspector SHALL NOT be compiled or available in release builds.

#### Scenario: Toggle inspector panel with action
Test: UI unit test verifying action registration and handler presence under debug assertions.

- **WHEN** `ToggleInspector` is dispatched in a debug build
- **THEN** the active window toggles its inspector state and refreshes the layout

#### Scenario: Release builds omit inspector
Test: workspace release compilation check `cargo check --release -p zeron`.

- **WHEN** building or checking the project in release mode
- **THEN** all inspector code and GPUI inspector API calls are omitted without compilation errors

### Requirement: Element picking and source location discovery

The inspector panel SHALL provide an interactive picking affordance that activates GPUI element picking mode, displays the active element's source location (`file:line`) and instance ID, and renders child inspector element states.

#### Scenario: Enter picking mode and inspect element
Test: UI visual validation in dev demo asserting panel, highlight, and source location rendering.

- **WHEN** the user clicks "Pick Element" in the inspector panel
- **THEN** the inspector enters picking mode (`inspector.is_picking() == true`)
- **AND** hovering elements highlights their hitboxes
- **AND** clicking an element displays its `source_location` (`file:line`) and `instance_id` in the inspector panel

#### Scenario: No element selected shows initial instruction state
Test: UI inspector renderer test when `active_element_id` is `None`.

- **WHEN** the inspector panel is open but no element has been picked
- **THEN** the panel shows instructions prompting the user to pick an element

### Requirement: Keymap persistence through shell re-application

The system SHALL bind `cmd-alt-i` to `ToggleInspector` within `shell::apply_keymap` under debug assertions so that the shortcut remains active after initial boot and keymap re-application.

#### Scenario: Shortcut survives keymap re-application
Test: UI unit test asserting keybinding is registered after `apply_keymap` runs.

- **WHEN** `shell::apply_keymap` executes
- **THEN** `cmd-alt-i` is registered as a key binding for `ToggleInspector` in debug builds
