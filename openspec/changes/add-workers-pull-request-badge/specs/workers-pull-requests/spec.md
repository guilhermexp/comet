## ADDED Requirements

### Requirement: Workers worktrees resolve their pull request

The Workers surface SHALL request change-request resolution for every project
that carries a worktree branch, addressed to the local device, and SHALL
request none for any other project. Resolution SHALL be gated by the same
`sidebar_show_pull_request` setting that gates the Orchestrator sidebar, and
SHALL reuse the existing checkout watch rather than opening a second one.

#### Scenario: Only worktrees are watched

Test: `workers_change_request_targets_cover_worktrees_only`

- **WHEN** the snapshot holds ordinary projects and worktree projects
- **THEN** the published targets name the worktree paths and branches only

#### Scenario: No worktree, no subscription

Test: `workers_change_request_targets_cover_worktrees_only`

- **WHEN** no project carries a worktree branch — the same empty set the model
  publishes while `sidebar_show_pull_request` is off
- **THEN** no target is published, so the reconcile opens no subscription

### Requirement: The row names the pull request beside the branch

A Workers project row that resolved a pull request for its checkout SHALL draw
the change-request badge beside its branch chip. A snapshot SHALL only be shown
against the checkout it was resolved for: both the working directory and the
branch SHALL match, so a branch switch drops the badge rather than showing the
previous branch's pull request.

#### Scenario: A stale branch does not keep the badge

Test: `change_request_for_checkout_requires_cwd_and_branch`

- **WHEN** a stored snapshot names a branch the project no longer has
- **THEN** the lookup returns nothing for that project
