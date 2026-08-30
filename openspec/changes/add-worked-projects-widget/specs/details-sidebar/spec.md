## ADDED Requirements

### Requirement: Worked Projects derivation maps touched registered projects

comet SHALL derive the list of registered projects touched by assistant tool calls in a Chat session, matching against leaf registered project roots and ordering them chronologically by first contact.

#### Scenario: Only the chat's own assistant tool calls are signal
Test: unit — non-assistant entries and unhandled tool calls are ignored.

- **WHEN** user messages, dispatched worker entries, or child subagent transcripts contain paths
- **OR** assistant tool calls use unhandled tool kinds such as `Mcp` or `Unknown`
- **THEN** those entries and tool calls do not contribute to the worked projects list
- **AND** only assistant tool calls from the current chat are considered

#### Scenario: Assistant tool call variants contribute touched paths
Test: unit — derivation across all supported tool call variants.

- **WHEN** an assistant invokes `ReadFile`, `WriteFile`, `EditFile`, `ApplyPatch`, `Search`, `Glob`, or `Exec`
- **THEN** candidate paths are extracted from each supported variant
- **AND** matching registered projects appear in the worked projects list

#### Scenario: Structured path fields are split only on the multi-root separator
Test: unit — derivation with spaced and parenthesised project folders.

- **WHEN** an assistant invokes `Search` or `Glob`, whose path field carries either one exact path or a `;`-delimited list of roots
- **THEN** the field is split on `;` alone, never scanned as free text
- **AND** a registered project whose folder name contains a space or ends in punctuation still matches
- **AND** only `Exec` commands, which are free prose, are scanned for path tokens

#### Scenario: The universe of worked projects is restricted to Registered Projects
Test: unit — derivation against registered projects list.

- **WHEN** assistant tool calls touch filesystem paths
- **THEN** only paths that match a registered project root or subfolder are included
- **AND** paths in unregistered folders never appear in the worked projects list
#### Scenario: Container roots are filtered out by the Leaf Root rule
Test: unit — candidate root filtering with ancestor containers.

- **WHEN** a registered project's path strictly contains another registered project's path
- **THEN** the ancestor container root is dropped from candidate matching
- **AND** only the leaf project is eligible to match paths under its root

#### Scenario: The Chat Checkout is excluded from candidate roots
Test: unit — derivation with own checkout path matching.

- **WHEN** assistant tool calls touch paths inside the chat's own checkout directory
- **THEN** the chat's own checkout project is excluded from candidate roots
- **AND** the chat checkout does not appear in the worked projects list

#### Scenario: Relative paths are ignored
Test: unit — derivation with relative tool call arguments.

- **WHEN** assistant tool calls provide relative paths or commands without absolute paths
- **THEN** those tokens are ignored
- **AND** only absolute (`/...`) or home-relative (`~/...`) paths are considered

#### Scenario: Home-relative paths are expanded when home directory is available
Test: unit — derivation with `~/` paths with and without home directory.

- **WHEN** an assistant tool call uses a `~/...` path
- **THEN** if a home directory is provided, `~` is expanded to the absolute home path
- **AND** if no home directory is provided, the token is discarded

#### Scenario: Path matching respects component boundaries
Test: unit — prefix matching with sibling folder names.

- **WHEN** an assistant touches `/path/to/project-a-sibling`
- **THEN** it does not match a registered project rooted at `/path/to/project-a`
- **AND** matches require exact equality or a `/` component delimiter after the root

#### Scenario: Projects are ordered chronologically by first contact
Test: unit — ordering of projects touched across multiple turns.

- **WHEN** multiple registered projects are touched at different points in the transcript
- **THEN** the returned worked projects list is ordered ascending by the time of first contact
- **AND** subsequent touches to an already-contacted project do not change its position

### Requirement: Workspace card displays Worked Projects in Details Sidebar

The Workspace card in the Details sidebar SHALL render a collapsible "Projects worked" section showing the count and list of worked projects when in Orchestrator mode.

#### Scenario: Worked projects section is visible only in Orchestrator mode
Test: none — sem harness de render; validação é visual.

- **WHEN** the Details sidebar is rendered for a chat
- **THEN** the "Projects worked" section is rendered only when `DetailsMode::Orchestrator` is active
- **AND** it is omitted in other sidebar modes

#### Scenario: Worked projects section is hidden when count is zero
Test: none — sem harness de render; validação é visual.

- **WHEN** a chat has touched zero candidate registered projects
- **THEN** the "Projects worked" section is completely omitted from the Workspace card

#### Scenario: Collapse state is persisted across context switches
Test: unit — sidebar preferences state round-trip for projects worked collapse token.

- **WHEN** the user toggles the collapse state of the "Projects worked" section
- **THEN** the expanded/collapsed state is persisted in `DetailsSidebarPreferences.expanded` keyed by context key
- **AND** reloading or switching context restores the chosen collapse state

#### Scenario: Clicking a worked project reveals it in Finder
Test: none — sem harness de render; validação é visual.

- **WHEN** the user clicks on a worked project row in the Workspace card
- **THEN** `WorkersModel::reveal_project` is invoked with the project's filesystem path

#### Scenario: Worked projects list is bounded in height with vertical scrolling
Test: none — sem harness de render; validação é visual.

- **WHEN** the worked projects section contains multiple project rows
- **THEN** the list container height is bounded to `WORKED_PROJECTS_VISIBLE_ROWS` rows
- **AND** additional rows are accessible via vertical scrolling without stretching the card
