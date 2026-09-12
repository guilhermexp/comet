# Spec: Workers Preset Model Selector

## ADDED Requirements

### Requirement: Preset Model Extraction
The system MUST parse a preset command string and detect if a model flag (`--model <value>`, `--model=<value>`, `-m <value>`, `-m=<value>`) is specified.
If present, the value MUST be identified as the active model.
If absent, the active model MUST default to "Default".

#### Scenario: Extract model from standard flag
- Given a command string `claude --model claude-sonnet-5`
- When extracted
- Then the active model is `claude-sonnet-5`
- Test: unit

#### Scenario: Extract model when no flag is present
- Given a command string `claude`
- When extracted
- Then the active model is `None` (Default)
- Test: unit

### Requirement: Preset Model Application
When a model is chosen from the selector:
1. If "Default" (or empty) is chosen, any existing model flag (`--model` or `-m` and its value) MUST be removed from the command string.
2. If a specific model is chosen:
   - If a model flag already exists in the command string, its value MUST be replaced in place.
   - If no model flag exists, `--model <model_id>` MUST be appended to the command string.
3. The updated command string MUST be saved to the preset via `model.update_preset`.
4. If the preset is currently being edited in the bottom input, the input text MUST also be updated.

#### Scenario: Select specific model for default command
- Given a preset with command `claude`
- When the user selects `Sonnet 5` (`claude-sonnet-5`)
- Then the preset command updates to `claude --model claude-sonnet-5`
- Test: unit

#### Scenario: Switch model on command with other flags
- Given a preset with command `codex --dangerously-bypass-approvals-and-sandbox --model gpt-5.5`
- When the user selects `GPT-5.6-Sol` (`gpt-5.6-sol`)
- Then the preset command updates to `codex --dangerously-bypass-approvals-and-sandbox --model gpt-5.6-sol`
- Test: unit

#### Scenario: Revert model to Default
- Given a preset with command `omp --model anthropic/claude-sonnet-5`
- When the user selects `Default`
- Then the preset command updates to `omp`
- Test: unit

### Requirement: Preset Model Popover UI
Each preset row in `render_presets` MUST render an interactive model button displaying the current model label with a vertical sort indicator (`[ <Model Name> ↕ ]`).
Clicking the button MUST open an anchored popover listing available models for that CLI.
The active model in the list MUST show a checkmark indicator.
Clicking an item in the list MUST apply the selection and dismiss the popover.
Clicking outside MUST dismiss the popover.

#### Scenario: Toggle and display model popover
- Given a rendered preset row for `claude`
- When the user clicks the model button
- Then the popover opens with available Claude models and the current active model marked
- Test: unit
