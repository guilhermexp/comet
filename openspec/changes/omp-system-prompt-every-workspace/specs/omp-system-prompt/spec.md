## ADDED Requirements

### Requirement: OMP chat runs append the system prompt in every workspace

The OMP runtime SHALL pass Comet's system prompt through `--append-system-prompt` on every chat run, regardless of the chat cwd. The delegation block SHALL be included only when the run registers the `workers` host tool. Model and command discovery launches SHALL NOT receive the append.

#### Scenario: Chat in an ordinary project

- **WHEN** an OMP chat runs with a cwd other than `~/.orchestrator`
- **THEN** the OMP process is launched with `--append-system-prompt` carrying the base prompt

#### Scenario: Workers tool not registered

- **WHEN** an OMP chat runs without the `workers` host tool
- **THEN** the appended prompt contains the base prompt and no delegation block

#### Scenario: Workers tool registered

- **WHEN** an OMP chat runs with the `workers` host tool
- **THEN** the appended prompt contains the base prompt followed by the delegation block
