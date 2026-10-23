# details-sidebar Specification

## Purpose

Worked Projects derivation, session inspection, and sidebar widgets in the Chat Details view.

## Requirements

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

### Requirement: Session Details identifies the host device
The Session card SHALL display a read-only Device row in its Project section, using the selected context's device name from the shared registry. An explicit remote device SHALL never be displayed as the current local machine. Long names SHALL remain contained with their full value available on hover.

#### Scenario: Inspect local or remote Chat
Test: none — native gpui render has no render harness; visual acceptance and existing context projection.
- **WHEN** the user inspects a Chat or selected project in Details
- **THEN** the Device row shows the same registered name used by the sidebar for that host
- **AND** selecting another host updates the displayed device

#### Scenario: Local Workers context
Test: none — native gpui visual acceptance; existing context projection defines local Workers ownership.
- **WHEN** Details inspects a Workers context with no explicit target device
- **THEN** the Device row identifies the local engine device

#### Scenario: Missing device metadata
Test: none — visual fallback review; render has no harness.
- **WHEN** the selected device has no registered display name
- **THEN** the row shows Unknown device without substituting a different host

### Requirement: Skill usage totals belong to the Project

Context sources SHALL report historical skill invocation totals across the retained Chats associated with the selected Project, including archived Chats and linked checkouts of that Project. Totals SHALL NOT depend on which of those Chats is open and SHALL NOT include independent CLI Worker transcripts or Chats belonging to another Project.

#### Scenario: Aggregate active and archived Chats
Test: integration — engine projection over persisted Chat Transcripts and the Project registry.

- **WHEN** two Chats of one Project record respectively two and three invocations of the same project skill and the second Chat is archived
- **THEN** the skill total is five when either Chat is inspected
- **AND** a new Chat in that Project sees the same total before recording its own invocations

#### Scenario: Respect Project identity across checkouts
Test: unit — association and relative skill identity projection.

- **WHEN** Chats use the principal checkout and a linked worktree associated with the same Project
- **THEN** their invocations of the same project-relative skill contribute to one Project total
- **AND** another Project with the same display name, skill name or adjacent folder does not contribute
- **AND** merely appearing in a Chat's Projects worked list does not associate that Chat with the Project

### Requirement: Skill totals measure recorded invocations

Skill totals SHALL derive only from identifiable skill invocations in assistant tool activity retained in the Chat Transcript. Each distinct recorded invocation SHALL contribute one use. Plain text mentions, user requests, file contents and reads of a skill's supporting documentation SHALL NOT be interpreted as invocations. The measure SHALL NOT claim that the agent followed the skill correctly.

#### Scenario: Identify skill activity without counting mentions
Test: unit — shared classification over structured tool calls.

- **WHEN** an assistant invokes an identifiable native skill tool or loads a skill entrypoint through a supported read tool
- **THEN** that activity is eligible for the matching skill's invocation count
- **AND** a user saying the skill name, an assistant mentioning it in prose or a read of its reference files contributes no use
- **AND** an activity whose skill identity is absent or ambiguous does not get assigned to an arbitrary skill

#### Scenario: Do not merge same-name skills by display text
Test: unit — matching by catalog identity and project-relative entrypoint.

- **WHEN** two distinct skills share a name or a global skill shares the name of a project skill
- **THEN** an explicit path identifies only the matching entrypoint
- **AND** the global skill's explicit path does not increment the project skill
- **AND** a name-only invocation contributes only when it resolves unambiguously in the applicable catalog

#### Scenario: Count invocation attempts rather than successful compliance
Test: unit — pending, successful and failed representations of a recognized call.

- **WHEN** an identifiable skill invocation is recorded before its result arrives
- **THEN** it contributes one recorded invocation attempt
- **AND** success or failure updates do not create another use or remove the attempt
- **AND** a bare `skill://name` is an entrypoint, while `skill://name/references/guide.md` is not

#### Scenario: Do not infer unobservable provider activity
Test: unit — non-signal tool calls and user content.

- **WHEN** a skill is only requested through the composer, read inside an opaque eval or shell command, or used inside an inline subagent's separate transcript
- **THEN** that alone does not contribute to this counter
- **AND** the counter is described as detected recorded invocations, not all possible skill usage

### Requirement: Skill history aggregation is replay safe

The system SHALL count one recorded tool invocation only once across its running, resolved, failed and replayed representations. Distinct invocation identities SHALL remain distinct even when they use the same skill in the same turn. Loading or reopening the sidebar SHALL NOT create a new use.

#### Scenario: Repeated snapshots do not inflate totals
Test: unit — aggregation with stable Chat and tool invocation identities.

- **WHEN** the same tool invocation is seen as a live call, a result, a persisted snapshot and a replay
- **THEN** its contribution remains one
- **AND** a second distinct call of the same skill contributes a second use

#### Scenario: Reload and restart preserve the reconstructed total
Test: integration — persisted history rehydration and stream reconnection.

- **WHEN** the sidebar is reopened, its watch reconnects or the engine restarts without new skill invocations
- **THEN** the reconstructed Project total is unchanged
- **AND** the computation does not require reading the Run Journal or private provider history

#### Scenario: Forked history is not new activity
Test: unit — fork and fork-of-fork transcript fixtures.

- **WHEN** a side Chat inherits its parent's transcript prefix and later records its own skill invocation
- **THEN** the inherited prefix contributes no additional uses
- **AND** only activity after that Chat's own fork boundary contributes new uses
- **AND** a fork of that side Chat does not recount either inherited prefix

### Requirement: Skill history coverage is explicit

A confirmed skill total SHALL include all retained Chat Transcripts of the selected Project available through the normal history synchronization path, without requiring the user to open those Chats. The system SHALL distinguish complete, loading, partial and unavailable coverage. Missing, unsupported or failed history reads SHALL NOT be converted to a confirmed zero or a complete total.

#### Scenario: Backfill existing cold history
Test: integration — history not mounted in the selected UI transcript.

- **WHEN** the Project already contains skill invocations in persisted Chats before the feature is installed
- **THEN** those invocations contribute after history loading completes, without manually opening each Chat
- **AND** a listed skill with no matching recorded invocation displays `x0` only once all eligible histories are covered

#### Scenario: Incomplete or unavailable history
Test: unit — snapshot coverage to visible state; integration — partial and failed history reads.

- **WHEN** history is still loading, some eligible Chat history is missing, or the host cannot provide the aggregation
- **THEN** the skill row shows `…` while loading or `—` for partial or unavailable coverage instead of a confirmed `xN`
- **AND** its tooltip explains the coverage state
- **AND** the skills list and file-opening action remain usable

### Requirement: Project skill totals refresh without changing context

After an identifiable skill invocation is recorded, the visible Project total SHALL refresh without reopening the sidebar or changing Chats. Switching Projects SHALL replace the aggregation scope and ignore stale results from the previous scope. Hiding the panel SHALL stop its UI-specific subscription without losing historical uses.

#### Scenario: A different Chat contributes a new use
Test: integration — Project watch includes a non-selected Chat.

- **WHEN** one Chat is displayed while another Chat of the same Project records a new invocation
- **THEN** the displayed Project total includes the new invocation after the history update is received
- **AND** the update is not limited to the currently selected Chat

#### Scenario: Switch Project during loading
Test: unit — scope generation and stale-response handling.

- **WHEN** the user switches from Project A to Project B while A's history is loading
- **THEN** only B's totals and coverage may appear beside B's skills
- **AND** a late result for A cannot overwrite B's display

### Requirement: Context sources presents compact historical counts

Each existing project skill row SHALL display a noninteractive `xN` label aligned to its right when coverage is complete, including `x0`. Counts SHALL retain the existing list order, row height, bounded scrolling and file-opening behavior. The header SHALL continue reporting the number of available skills, not the sum of invocations. Counts SHALL use the existing theme and typography.

#### Scenario: Inspect a used and an unused skill
Test: unit — display-state derivation; none — native appearance, tooltip and file-opening gesture require visual acceptance.

- **WHEN** the expanded list has complete totals of twelve for brainstorming and zero for agents-box
- **THEN** their rows show `brainstorming` with `x12` and `agents-box` with `x0`
- **AND** the count tooltip identifies the value as recorded uses in this Project
- **AND** clicking the skill row still opens its existing skill file

#### Scenario: Preserve list geometry and readable values
Test: none — native visual acceptance at narrow and normal Details widths in light and dark themes.

- **WHEN** a skill name is long or its counter has several digits
- **THEN** the name truncates before the counter, which remains legible without overlap or wrapping
- **AND** rows keep their current height and the list retains its seven-row viewport limit
- **AND** no ranking, filters, reset control, graph or additional panel is introduced
