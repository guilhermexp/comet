# Change: Add Model Selector to Workers Presets

## Why

In the Workers settings panel (`Settings > Presets`), agent presets (e.g. `omp`, `claude`, `codex`, `pi`, `opencode`) only show the raw command string, a toggle, and quick launch. Selecting or changing the model used by a preset requires manual command-line argument edits (`--model <name>`), without visibility into what model is currently active or what models are available for that CLI. Users need an interactive model selector on each preset row that displays the active model, lists available models in a dropdown menu, and updates the command line automatically so the CLI executes with the chosen model.

## What Changes

- Add model extraction (`extract_model_from_command`) and application (`apply_model_to_command`) helpers to parse and update `--model` / `-m` flags cleanly on preset command lines.
- Provide curated static catalogs of known models for standard CLIs (`claude`, `codex`, `omp`, `pi`, `opencode`, `cursor`, `agy`) and support dynamic resolution via the engine's `LIST_MODELS` RPC.
- Render a compact model selector dropdown button (`[ <Model Name> ↕ ]`) on each preset row in `crates/ui/src/workers/settings.rs`.
- Clicking the button opens an anchored popover listing available models with active checkmark indicators.
- Selecting a model updates the preset command immediately, synchronizes with any active preset edit input, and persists via `model.update_preset`.

## Capabilities

### New Capabilities

- `workers-preset-model-selector`: Interactive model picker on Workers preset rows that inspects, selects, and updates the `--model` flag on agent CLI commands.

### Modified Capabilities

None.

## Impact

- `crates/ui/src/workers/settings.rs`: Model selector UI, popover menu, command parsing and updating logic.
- `crates/ui/src/workers/model.rs`: Expose `state` accessor for engine RPC model querying.
