## MODIFIED Requirements
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
