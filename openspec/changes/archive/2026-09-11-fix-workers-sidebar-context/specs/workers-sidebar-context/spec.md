## Purpose
Make Worker checkout identity, current branch and confirmed pull requests readable while keeping empty inactive projects out of the sidebar.

## ADDED Requirements
### Requirement: Consistent checkout context
The sidebar SHALL show checkout type and current branch at project level, using the same branch for PR lookup. Ordinary local checkouts SHALL be eligible for confirmed PR badges. Session runtime icons SHALL remain independent of Git context.

#### Scenario: Branch switches update context consistently
Test: unit — WorkersProject and UI presentation/PR targets.
- **WHEN** the disk branch differs from the launch or worktree creation branch
- **THEN** the project header and PR lookup use the disk branch

#### Scenario: Ordinary checkout has a PR
Test: unit — UI PR target projection.
- **WHEN** a working-set local checkout has a branch and is not a worktree
- **THEN** its confirmed PR can appear beside the checkout context

#### Scenario: Readable native layout
Test: none — native demo screenshot; no automated GPUI render harness.
- **WHEN** projects with local and worktree checkouts are shown
- **THEN** checkout type and branch are readable without hovering and session rows do not repeat branch glyphs

### Requirement: Empty inactive projects do not remain selected automatically
The model SHALL NOT select a project merely because it is first in the registry. When the selected session is removed or archived without a remaining sibling, its implicit project selection SHALL clear. Explicit project/launcher navigation SHALL remain supported without deleting project records.

#### Scenario: Empty registry project at startup
Test: unit — Workers model selection and sidebar working-set projection.
- **WHEN** an unselected empty project is first in the registered list
- **THEN** it is not retained as a sidebar row

#### Scenario: Last session leaves
Test: unit — Workers model selection and sidebar working-set projection.
- **WHEN** the selected project's final session is archived or removed
- **THEN** its implicit project selection clears and the empty project disappears from the working set

#### Scenario: Explicit launch target remains accessible
Test: unit — Workers model selection and sidebar working-set projection.
- **WHEN** the user explicitly opens an empty project or its launcher
- **THEN** it remains available for starting a session
