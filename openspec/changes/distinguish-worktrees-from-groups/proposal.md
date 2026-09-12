# Change: A worktree is a checkout, not an organizational group

## Why

`create_worktree` registers the worktree as a child project carrying
`is_folder: true` (it nests under its parent in the sidebar) plus
`worktree_branch`. The `comet-local` projection then marks every project with
`is_folder && parent_id.is_some()` as `isGroup`, so a worktree reaches the UI
as an organizational group. Measured on the real route: a worktree created
through `LocalWorkersClient::create_worktree` comes back from `bootstrap()`
with `is_group = true`.

The upstream Swift app never had this defect — `Models.swift:43` spells the
same predicate as `parentProjectID != nil && worktreeBranch == nil && isFolder
== true`. The Rust port of that projection dropped the branch clause, and this
crate already spells it correctly in `remove_group` (`lib.rs:1973`), which
refuses a worktree as a group. Only the projection disagrees.

Because a group owns no checkout of its own, the UI denies groups everything
that names a path — and a worktree inherits every denial:

- clicking the row does not select the project (`workspace.rs:785`)
- the hover controls (terminal, new session) are hidden (`workspace.rs:846`)
- `open_launcher` returns early, so the launcher never opens (`model.rs:919`)
- an empty worktree renders no `No sessions yet.` (`workspace.rs:917`)
- the project ledger drops it (`lib.rs:1088`), against the contract written
  directly above that filter — so it is absent from Settings › Projects and
  from the composer's `@` project menu
- Worked Projects in the Details sidebar cannot match it
  (`worked_projects.rs:42`)

Compounded, a worktree created without a session is invisible: the sidebar
keeps a session-less project only while it is the selected or the launcher
project, and the group flag blocks both. `New worktree…` creates the checkout
on disk, runs setup, registers the project — and no row ever appears.

No test caught it: the sidebar and menu tests build `WorkersProject` by hand
with `is_group: false` alongside `worktree_branch: Some(..)`, a combination the
`comet-local` route never produces. Same class as the recorded `git_branch`
gotcha.

## What Changes

- The host projection marks a project as a group only when it has no worktree
  branch, matching the canonical predicate.
- The worktree lifecycle test asserts the flag on the bootstrap the UI reads,
  so the projection cannot silently regress.

Restoring the flag restores selection, hover controls, launcher, empty-state
row, ledger membership and Worked Projects matching by itself: each of those
already keys off `is_group` and needs no edit.

## Capabilities

### New Capabilities

- `workers-project-identity`: how a Workers project reaches the UI as a group,
  a worktree, or a plain checkout.

### Modified Capabilities

None.

## Impact

- `third_party/unpeel/crates/unpeel-core/src/controller_host.rs`: one clause in
  the snapshot projection.
- `crates/workers-unpeel/tests/project_actions.rs`: one assertion.
- Out of scope: `unpeel-tui/src/sessions.rs:1712` carries the same missing
  clause on the TUI host's own projection. It is not on the `comet-local`
  route; left untouched rather than edited without evidence against comet.
