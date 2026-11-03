# Spec Delta

## MODIFIED Requirements

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

## ADDED Requirements

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
