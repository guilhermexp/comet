# Workers Sidebar Context Specification

## Purpose
Make Worker checkout identity, current branch and confirmed pull requests readable while keeping empty inactive projects out of the sidebar.

## Requirements
### Requirement: Consistent checkout context
The sidebar SHALL show each base project as a folder containing its local Workers and nested worktree rows. A worktree row SHALL use the existing worktree icon instead of a folder icon and display its current branch name, with its Workers underneath. Base projects SHALL NOT show Local/main or checkout subtitles. Confirmed PRs SHALL use an icon and tooltip on their corresponding checkout. PR lookup and displayed worktree names SHALL use the same current branch. Runtime icons SHALL remain independent of Git context.

#### Scenario: Branch switches update context consistently
Test: unit — existing WorkersProject and UI presentation/PR targets.
- **WHEN** the disk branch differs from the launch or worktree creation branch
- **THEN** the worktree label and PR lookup use the disk branch

#### Scenario: Ordinary checkout has a PR
Test: unit for existing PR target projection; native QA for icon rendering.
- **WHEN** a working-set local checkout has a confirmed PR
- **THEN** its PR icon can appear on the project row without adding a Local/main subtitle

#### Scenario: Readable native layout
Test: none — native demo screenshot; no automated GPUI render harness.
- **WHEN** a project has both local Workers and a linked worktree with Workers
- **THEN** the base retains its folder icon, local Workers sit directly inside it, and the nested worktree uses the worktree icon and branch name with its Workers underneath
- **AND** no Local/main subtitle or repeated per-Worker Git glyph is shown

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
