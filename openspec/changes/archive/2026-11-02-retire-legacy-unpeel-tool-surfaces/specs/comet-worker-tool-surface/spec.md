## ADDED Requirements

### Requirement: Comet Workers have no legacy Browser or Computer engine
Workers launched or resumed by Comet SHALL NOT be granted, advertised or able to call the vendored `browser` or `computer` tool, whatever their inherited configuration or legacy manifest says. Comet SHALL NOT route the legacy browser or computer cleanup entry points. Standalone Unpeel defaults and engines SHALL remain unchanged, and `comet-computer-use` SHALL keep its own registration and authorization.

#### Scenario: New Worker without inherited configuration
Test: unit — vendored launch policy fixture.
- **WHEN** Comet launches a Worker and no inherited browser access is configured
- **THEN** the session manifest records no browser or computer grant
- **AND** the Sessions grant remains enabled

#### Scenario: Resumed legacy manifest
Test: unit — vendored manifest resume fixture.
- **WHEN** a Worker manifest written with browser and computer grants is resumed under Comet
- **THEN** the resumed session has neither grant

#### Scenario: Direct legacy call
Test: unit — vendored MCP host fixture.
- **WHEN** a Comet Worker calls `browser` or `computer`, including `help` or `context`
- **THEN** the call is refused with a removal message and no engine code runs
- **AND** `tools/list` lists neither tool

#### Scenario: Standalone Unpeel
Test: unit — vendored launch policy fixture without the Comet policy.
- **WHEN** the Comet policy is not registered
- **THEN** browser access keeps its standalone default

### Requirement: Obsolete tool aliases are not callable
The Workers MCP host SHALL reject `browser_*` names, `list_sessions`, standalone `read_transcript` and the other legacy session names, and `list_children`, `wait_for_children`, `summarize_children` and `report_to_parent`, as tool names and as `sessions` actions, without side effects. Canonical `sessions` actions (including `read_transcript`) and `list_group`, `wait_for_group`, `summarize_group` and `report_to_group` SHALL keep working with explicit peer and group targets.

#### Scenario: Removed familial name
Test: unit — vendored MCP host fixture.
- **WHEN** a Worker calls `report_to_parent` or `sessions` with action `list_children`
- **THEN** the call fails, names the canonical group operation, and nothing is delivered

#### Scenario: Canonical replacement
Test: unit — vendored MCP host fixture.
- **WHEN** a Worker calls `sessions` with action `read_transcript` or `list_group`
- **THEN** the canonical behavior and authorization apply unchanged

#### Scenario: Removed browser alias under Comet
Test: smoke — real `zeron __mcp__` over stdio (disposable).
- **WHEN** a Comet Worker calls a removed `browser_*` name such as `browser_open`
- **THEN** the call fails without side effects and points to the app's integrated Browser panel, never to the retired `browser` tool; standalone Unpeel still names the canonical `browser` call
