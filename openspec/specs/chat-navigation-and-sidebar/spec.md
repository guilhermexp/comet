# chat-navigation-and-sidebar Specification

## Purpose

Organize durable Chats by their source context and make frequent Chat navigation available through predictable keyboard shortcuts.

## Requirements

### Requirement: Sidebar views organize durable Chats consistently

The sidebar SHALL support conversation-aware organization and sorting derived from durable Chat source context without changing Session execution state.

#### Scenario: Two Chats share a checkout but have different source branches

Test: proto/engine unit tests plus sidebar projection test.

- **WHEN** two Chats carry distinct source contexts under the same Space
- **THEN** organization and sorting keep their durable Chat identities separate
- **AND** legacy Chats without source context remain visible through a deterministic fallback

### Requirement: Keyboard shortcuts navigate and archive Chats safely

The desktop app SHALL provide configurable shortcuts for cycling, jumping to visible Chat slots, and archiving the selected eligible Chat.

#### Scenario: Modified digit matches a Chat jump slot

Test: UI shortcut unit test against the exact sidebar row projection.

- **WHEN** the configured modifier and digit are pressed outside a conflicting popover or question panel
- **THEN** the corresponding visible Chat is selected
- **AND** the key is not interpreted as an answer or model-picker action

#### Scenario: Archive shortcut targets the active Chat

Test: UI unit test with active, missing, and already archived Chat states.

- **WHEN** the archive shortcut is pressed for an open archivable Chat
- **THEN** that Chat is archived exactly once
- **AND** no unrelated Chat or live Session is interrupted

### Requirement: Sidebar identifies worked root projects for each Chat

The Sessions sidebar SHALL display the root project names worked in each Chat beside its branch metadata. It SHALL use the existing Projects worked definition and registered project identity, deduplicate all matched checkouts of one project, and preserve first-contact order. Names SHALL NOT be inferred from branch prefixes, worktree folder names, the Chat title, or the currently selected Chat.

#### Scenario: Multiple branches belong to one worked project
Test: unit — path attribution and sidebar projection.

- **WHEN** one Chat worked in the Comet principal checkout and several linked Comet worktrees
- **THEN** its sidebar metadata shows one folder label named `comet`
- **AND** it shows no worktree tree, additional branch names, diff totals, or duplicate project label

#### Scenario: Two projects use the same branch name
Test: unit — registered identity projection with colliding branch names.

- **WHEN** a Chat worked in two distinct registered projects with identical branch names
- **THEN** both root project names appear in first-contact order, once per project identity
- **AND** repeated tool contact does not reorder them

#### Scenario: Projects belong to the Chat that worked in them
Test: integration — historical summaries for selected and unselected Chats; unit — sidebar row projection.

- **WHEN** Chat A worked in Comet and Chat B worked in another project, with B never opened in the current UI process
- **THEN** each row displays only its own worked project names
- **AND** switching selection or closing Details does not transfer, clear, or replace the other Chat's labels

#### Scenario: Existing attribution boundaries remain unchanged
Test: unit — existing path derivation regressions shared with the new consumer.

- **WHEN** tool activity includes the Chat's own checkout, user-authored text, unsupported tools, relative paths, unregistered folders, or paths crossing a project-name boundary
- **THEN** attribution follows the existing Projects worked rules rather than adding projects from those nonmatching inputs
- **AND** paths belonging to another device are not matched against this device's checkout catalog

### Requirement: Worked project labels share the existing metadata presentation

Worked project labels SHALL reuse the existing folder icon, typography and one-line metadata layout without changing Chat selection, ordering, drag targets, status, PR actions or the Details panel. The normal row SHALL allocate metadata height when labels exist even without a branch. Compact SHALL retain its existing no-metadata-row policy. A short branch SHALL use its content width so the first project follows at the existing local gap.

#### Scenario: Label remains independent of Branch and Location
Test: unit — GPUI row geometry and menu interaction; none — native visual smoke.

- **WHEN** Projects worked is enabled and Branch or Location is disabled, or the Chat has no branch
- **THEN** the normal row still shows its worked projects without an empty branch glyph or branch separator
- **AND** the row has exactly the metadata height required by its visible content

#### Scenario: Compact and Archived preserve existing row behavior
Test: unit — GPUI geometry and active/archived interaction.

- **WHEN** the user switches Compact on
- **THEN** the sidebar keeps the current compact height and hides the metadata row, including worked project labels
- **WHEN** a normal-layout archived Chat has known worked projects
- **THEN** it uses the same root-project labels and sizing rules as other Chats without changing its unarchive behavior

#### Scenario: Long or multiple labels fit the sidebar
Test: unit — GPUI bounds and preserved PR interaction; none — native narrow/wide captures.

- **WHEN** a normal row has multiple worked projects or a long project name in a narrow sidebar
- **THEN** labels stay on one line within the available metadata width using the current overflow treatment
- **AND** the complete project names remain available on hover
- **AND** branch, PR, status and time controls are not overlapped or displaced outside the row

#### Scenario: A short branch does not reserve an empty metadata column
Test: unit — rendered GPUI bounds with Branch and Projects worked enabled; none — native visual smoke.

- **WHEN** a normal Chat row shows a short branch such as `main` followed by a worked project
- **THEN** the first project folder label starts immediately after the branch content and the established small metadata gap
- **AND** widening the sidebar does not create an expanding empty area between those labels
- **AND** archived rows use the same adjacency rule

### Requirement: Worked project visibility is a persistent Show preference

The sidebar menu SHALL expose a `Projects worked` toggle under `… > Show`, enabled by default and persisted like the existing metadata preferences. Its state SHALL be independent of Branch, Pull request, Harness, Location and Details visibility. Existing settings files without the new field SHALL load with the toggle enabled without changing neighboring preferences.

#### Scenario: User toggles worked projects without closing Show
Test: unit — production GPUI menu click, rendered rows and settings reload.

- **WHEN** the user clicks `… > Show > Projects worked`
- **THEN** only the worked project labels change visibility immediately
- **AND** the submenu remains open, like the other Show toggles
- **AND** the chosen value survives settings flush and application reload

#### Scenario: Old settings and later unrelated saves preserve the preference
Test: unit — central settings writer, legacy load and stale-copy merge.

- **WHEN** an existing settings file lacks the new preference
- **THEN** Projects worked is enabled and existing neighboring settings remain unchanged
- **WHEN** another settings surface saves after the user changes Projects worked visibility
- **THEN** that later save does not revert the chosen visibility

### Requirement: Worked project summaries do not require opening every Chat

The system SHALL derive worked project identities outside UI render from retained Chat history, make historical results available without selecting each Chat, and refresh them when history or project associations change. It SHALL retain available results across restart and host disconnection without keeping every transcript open. Missing or partial history SHALL NOT fabricate a project or erase known identities as a confirmed empty result.

#### Scenario: Cold restart displays historical projects
Test: integration — cold persisted snapshots and registry round-trip.

- **WHEN** the application starts with retained history for several Chats
- **THEN** the corresponding root-project labels become available without opening those Chats
- **AND** render consumes compact summaries instead of loading, scanning or pinning each full transcript

#### Scenario: New tool contact updates a background Chat
Test: integration — history invalidation and nonselected Chat summary.

- **WHEN** new supported tool activity touches another registered project in a nonselected Chat
- **THEN** its summary and sidebar labels refresh without changing selection or reopening Details
- **AND** unrelated Chats keep their own summaries

#### Scenario: Project name or association changes
Test: integration — catalog invalidation; unit — live name resolution.

- **WHEN** a known project is renamed or a linked checkout's project association changes
- **THEN** the sidebar resolves the current registered name and attribution without waiting for another tool call
- **AND** it never infers the new association from a branch or folder name

#### Scenario: Host offline, old host or incomplete history
Test: integration — optional metadata, incomplete snapshots and host-scoped projection.

- **WHEN** the host is offline but a summary was already synchronized
- **THEN** the sidebar retains that Chat's known labels
- **WHEN** an old host supplies no summary or history cannot be read completely
- **THEN** unknown attribution does not appear as another Chat's projects or a fabricated complete empty result
- **AND** the Chat remains usable and known labels are not erased because of the failed or incomplete read

### Requirement: Chat title bar presents compact mention labels

The Chat title bar SHALL present supported mentions as compact readable labels without exposing their Markdown destination or full path. It SHALL reuse the established title projection and preserve its current single-line truncation, typography, harness identity and controls. Presentation SHALL NOT rewrite the durable title or the text used for an explicit rename.

#### Scenario: A project mention has the same readable identity in sidebar and title bar
Test: unit — production title-bar projection with synthetic Chat state; none — native visual smoke.

- **WHEN** the stored title contains a project mention whose label is `studio` and whose destination is a full project path
- **THEN** the title bar shows `studio` instead of the raw Markdown and destination path
- **AND** the stored title remains byte-identical

#### Scenario: Legacy cut mentions and ordinary manual text remain readable
Test: unit — existing complete/cut mention presenter plus title-bar consumer.

- **WHEN** a title contains a legacy cut mention, Unicode text or ordinary square brackets
- **THEN** the title bar follows the established readable title projection without dropping ordinary text or exposing the cut destination
- **AND** a title without mentions keeps its existing wording

#### Scenario: Long compact titles preserve title-bar controls
Test: unit — GPUI title-bar bounds; none — native narrow/wide captures.

- **WHEN** a long title with project mentions is shown in a narrow or wide window
- **THEN** the title remains a single truncated line within the available title area
- **AND** the harness icon, navigation, window drag region and existing action controls retain their space and behavior
