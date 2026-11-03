# Spec Delta

## ADDED Requirements

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

Worked project labels SHALL reuse the existing folder icon, typography and one-line metadata layout without changing Chat selection, ordering, drag targets, status, PR actions or the Details panel. The normal row SHALL allocate metadata height when labels exist even without a branch. Compact SHALL retain its existing no-metadata-row policy.

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
