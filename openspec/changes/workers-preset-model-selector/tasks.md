# Tasks: Add Model Selector to Workers Presets

## 1. Command & Catalog Logic

- [ ] Implement `extract_model_from_command` and `apply_model_to_command` in `crates/ui/src/workers/settings.rs`
- [ ] Add unit tests verifying model extraction and application across various flag patterns (`--model`, `-m`, `--model=`, `-m=`, default)
- [ ] Implement `static_models_for_cli` and `available_models_for_preset` with support for dynamic engine resolution

## 2. UI Integration

- [ ] Add `open_model_menu_preset_id` and `dynamic_models` fields to `WorkersSettingsView`
- [ ] Expose `state()` accessor on `WorkersModel`
- [ ] Render the model selector button `[ <Model Name> ↕ ]` on each preset row in `render_presets`
- [ ] Render the anchored popover menu with model items and checkmarks
- [ ] Wire model selection to update the preset's command line via `model.update_preset` and sync the active composer input

## 3. Verification

- [ ] Format all workspace files with `cargo fmt --all`
- [ ] Run targeted unit tests
- [ ] Validate OpenSpec change with `openspec validate`
