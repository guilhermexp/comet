## ADDED Requirements

### Requirement: Specialized operational output stays consistent and inspectable
The transcript SHALL apply the compact event typography and inline hover disclosure contract to file edits, commands, searches, MCP calls, generic calls, tasks and reasoning. Expanded payloads SHALL use bounded code or diff surfaces, and failures SHALL retain their recorded details even when specialized previews are absent.

#### Scenario: Failed specialized tool
Test: unit — projection and height; none — native GPUI visual acceptance.
- **WHEN** a file or task tool fails with a recorded diagnostic
- **THEN** its diagnostic remains available by expansion
- **AND** its header indicates failure using the common event presentation

#### Scenario: Mixed specialized output
Test: none — native GPUI visual acceptance; unit for labels and height budgets.
- **WHEN** the user expands command, MCP, search, edit, task and reasoning output
- **THEN** headers share typography and inline hover-only arrows
- **AND** code and diffs retain syntax and internal scrolling without changing execution state

### Requirement: Repeated presentation within a turn is suppressed
The transcript SHALL display an identical standalone error or image path only once within an assistant entry. Successful task snapshots with no changed items SHALL not create empty disclosures. Distinct errors, images, failed task calls and subsequent turns SHALL remain visible.

#### Scenario: Tool and text repeat an image
Test: unit — transcript projection.
- **WHEN** a tool and narrative text reference the same image path within an entry
- **THEN** one inline image is displayed
- **AND** a different path or later entry remains independently visible

#### Scenario: Error and terminal error repeat
Test: unit — transcript projection.
- **WHEN** two standalone error parts have the same diagnostic in an entry
- **THEN** one error presentation is displayed
- **AND** a distinct diagnostic remains visible

#### Scenario: Unchanged successful task list
Test: unit — task projection.
- **WHEN** a successful task update repeats the previous list
- **THEN** it creates no empty task disclosure
- **AND** duplicate task titles do not hide changes to separate occurrences

### Requirement: Native content presentation preserves semantics
The transcript SHALL present narrative, code, user attachments and subagent summaries using consistent typography while preserving native links, previews, real child status and source data. Reasoning whose entire body equals its compact title SHALL not repeat that title in an empty expansion.

#### Scenario: Reasoning contains only its title
Test: unit — reasoning body policy; none — native GPUI visual acceptance.
- **WHEN** reasoning has one plain paragraph already shown in full as the header
- **THEN** it has no redundant detail disclosure
- **AND** longer or structured reasoning remains expandable

### Requirement: Activity and narrative retain visual hierarchy
Expanded turn activity SHALL preserve the transcript spacing between narrative blocks and operational groups. Consecutive operational rows SHALL remain compact. File details SHALL use an integrated 28px header and unified diff card with old/new line gutters, a preview bounded to 260px and explicit expansion and mixed-tool summaries SHALL use a neutral activity icon. Failed calls SHALL not be labelled as successful creation or execution.

#### Scenario: User opens completed activity
Test: unit — narrative boundary gaps, file preview heights and unified line gutters; none — native GPUI visual acceptance.
- **WHEN** the user opens a completed turn containing narrative, tools and file changes
- **THEN** context changes have visible spacing while tool rows stay compact
- **AND** file previews remain bounded until expanded, with header and code in one frame and no fabricated line positions for truncated tails
- **AND** the activity block is visually distinct from the final response

## MODIFIED Requirements

### Requirement: Disclosure arrows follow event text
The transcript SHALL place event disclosure arrows immediately after their content, using text-width labels that can shrink and truncate in constrained columns. JavaScript and Python eval rows SHALL omit a redundant language prefix when their icon already identifies the language.

#### Scenario: Short and long event labels
Test: none — native GPUI visual acceptance.
- **WHEN** a tool, reasoning, task or turn summary has an expansion arrow
- **THEN** the arrow follows its text instead of occupying the far column edge
- **AND** long labels truncate without clipping the arrow

#### Scenario: JavaScript eval has a title
Test: unit — stream_copy projection.
- **WHEN** a JavaScript or Python eval tool has a descriptive title
- **THEN** the row shows the title without a duplicate language prefix
- **AND** non-eval text and unrecognized language labels remain intact
