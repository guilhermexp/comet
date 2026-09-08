# Tasks: Add Model Selector to Workers Presets

## 1. Command & Catalog Logic

- [x] Implement `extract_model_from_command` and `apply_model_to_command` in `crates/ui/src/workers/settings.rs`
- [x] Add unit tests verifying model extraction and application across various flag patterns (`--model`, `-m`, `--model=`, `-m=`, default)
- [x] Implement `static_models_for_cli` and `available_models_for_preset` with support for dynamic engine resolution

## 2. UI Integration

- [x] Add `open_model_menu_preset_id` and `dynamic_models` fields to `WorkersSettingsView`
- [x] Expose `state()` accessor on `WorkersModel`
- [x] Render the model selector button `[ <Model Name> ↕ ]` on each preset row in `render_presets`
- [x] Render the anchored popover menu with model items and checkmarks
- [x] Wire model selection to update the preset's command line via `model.update_preset` and sync the active composer input

## 3. Verification

- [x] Format all workspace files with `cargo fmt --all`
- [x] Run targeted unit tests
- [x] Validate OpenSpec change with `openspec validate`
