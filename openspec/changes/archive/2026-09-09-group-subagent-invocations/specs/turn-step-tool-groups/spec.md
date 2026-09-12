## ADDED Requirements
### Requirement: Sibling subagents share one visual row
Multiple adjacent subagents belonging to the same recorded task invocation SHALL render individual clickable pairs of a larger (28px) avatar followed immediately by its own name in one wrapping row with a shared lifecycle summary. Different task invocations and unassociated single spawns SHALL remain separate. Each avatar and name SHALL open its own subagent transcript. Running and failed children SHALL remain distinguishable; completing the parent SHALL NOT mark running children completed. Per-child identities and aggregate activity counts SHALL remain unchanged.

#### Scenario: Fan out several children
Test: unit — transcript projection; none — native GPUI avatars, wrapping and clicks.
- **WHEN** a task invocation starts two or more linked subagents
- **THEN** their avatars and names share a row
- **AND** each child remains independently navigable
- **AND** another invocation is not merged into that row

### Requirement: Preview identity and file card surfaces match their source
Subagent preview tabs SHALL show the same doc-keyed avatar as their transcript link, with activity shown separately. File change cards SHALL use the same neutral background as command cards, retaining semantic diff washes and rounded clipping.

#### Scenario: Open a subagent preview
Test: none — native GPUI visual validation.
- **WHEN** the user opens a subagent
- **THEN** its preview tab displays that subagent's avatar

#### Scenario: Render an edit card
Test: none — native GPUI visual validation.
- **WHEN** a file edit is rendered beside command cards
- **THEN** their neutral backgrounds match without a black fill
