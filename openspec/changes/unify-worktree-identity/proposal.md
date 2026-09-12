# Change: One verdict per project, and a worktree that works like one

## Why

Two changes taught the `comet-local` projection to recognise a worktree:
`distinguish-worktrees-from-groups` stopped it from projecting as a group, and
`adopt-external-worktrees` let disk fill the branch and parent the registry
never recorded. Both changed what the sidebar SHOWS. Neither changed what the
rest of the app DOES with the row, and the callers still each ask their own
question:

- **Launch.** `resolve_host_create` rejects a project the creation catalog
  marks as a folder ("project is a folder", `controller_api.rs:520`), and the
  catalog is built from `project.is_folder && parent_id.is_some()` — which is
  true for every worktree the app created, because `create_worktree_at` writes
  `is_folder: true`. An adopted worktree fails the next gate instead: the
  catalog clones `worktree_branch` straight from the registry, so it is `None`
  where the UI echoed the branch it read from the snapshot, and the
  compatibility assertion answers "worktree does not belong to project". Either
  way `create_worktree_and_launch` then runs its rollback and deletes the
  checkout it just created.
- **Two verdicts on one record.** The projection decides `isGroup` and
  `worktreeBranch` in two independent places on the same object. `create_group`
  stores the PARENT's path in the group record (`lib.rs:1700-1713`), and the
  menu offers "New group…" on any root row — including an adopted worktree. So
  a group hanging off a worktree gets a branch borrowed from its parent's
  checkout, is offered "Remove worktree", and hands `worktrees::remove` the
  PARENT's path: removing a label deletes a working tree.
- **Removal.** `remove_worktree` refuses any project without
  `worktree_branch` in the REGISTRY, so an adopted worktree — whose identity
  came from disk — cannot be removed at all. Rename is the same story routed
  the other way: an adopted worktree reaches `rename_group_project` and takes
  "only plain groups can be renamed here".
- **Detached HEAD.** `git_checkout` reports the 7-char sha as the branch, and
  the projection promotes it to `worktreeBranch`. A sha is not a branch name:
  it draws as a branch chip and signs a pull-request lookup.
- **The PR badge.** `worktree_branch` is the branch the worktree was CREATED
  on. A `git switch` inside the checkout does not update it, so the badge
  points at a branch that is no longer checked out and quietly disappears.
  `git_branch` already carries the branch on disk at every bootstrap and had no
  reader at all.

## What Changes

- The projection computes ONE verdict per project, before the object is built:
  `is_group` from the canonical registry predicate, and the effective worktree
  branch as registry-first, disk second, gated on `!is_group` and on a HEAD
  that names a branch. Registry precedence is unchanged — detection still only
  fills what is absent.
- The creation catalog carries that verdict: a worktree is not a folder, and it
  carries the path and branch the UI reads from the same snapshot, so the
  launcher's payload matches and the launch goes through.
- `worktree_branch` in the registry stops meaning "this is a worktree" and
  starts meaning "the app created this checkout, so the app may delete it".
  Deleting from disk needs that record AND
  `unpeel_core::worktrees::is_managed`; without both, removal only
  de-registers the project and the folder stays.
- A failed launch no longer deletes the worktree it just created — the same
  policy already written fifteen lines above for a failed setup.
- Group, rename and remove routing all ask one shared predicate instead of
  three hand-rolled copies of it.
- The pull-request badge is GATED by `worktree_branch` and VALUED by
  `git_branch`, in both halves of the pair (the subscription target and the
  read), so it follows the checkout.

## What this does NOT change

- No new wire field, no registry migration, no write from a read route.
  `create_worktree_at` keeps writing `is_folder: true` — that flag simply
  becomes an inert hint on this route.
- Projection precedence stays registry-first, as shipped in
  `adopt-external-worktrees`. The badge is fixed in its CONSUMER; making the
  projection disk-first is a separate change.
- `controller_api.rs` is untouched: the compatibility assertion that stops a
  Controller from injecting a path stays exactly as it is.

## Capabilities

### Modified Capabilities

- `workers-project-identity`: one verdict per project, and every caller that
  acts on a worktree — launch, remove, rename, badge — reads it.

The capability is not in `openspec/specs/` yet, and three live changes write to
it: `distinguish-worktrees-from-groups` (which declares it New),
`adopt-external-worktrees`, and this one. Whichever archives first creates the
spec, so this delta is split to say what it adds and what it replaces:

- ADDED — launch, removal ownership and the pull-request badge. No other change
  states them.
- MODIFIED `A worktree never projects as a group` — the group predicate from
  `distinguish-worktrees-from-groups`, restated as one verdict computed before
  the object is built, and extended to the removal, menu and confirmation
  routing that reads it.
- MODIFIED `A linked worktree is recognised from its checkout` — disk detection
  from `adopt-external-worktrees`, whose unqualified "report the checked-out
  branch as the project's worktree branch" is the clause a detached HEAD breaks.
  Precedence is unchanged: registry first, disk fills only what is absent.

Archive order is therefore `distinguish-worktrees-from-groups` →
`adopt-external-worktrees` → this change. `openspec validate --strict` already
refuses the MODIFIED delta while the spec is absent, so the order enforces
itself.

## Impact

- `third_party/unpeel/crates/unpeel-core/src/controller_host.rs`: `detached` on
  `GitCheckout`; one verdict hoisted above the object build in
  `DiskCatalog::capture`; three fields on `HostCreateProject`.
- `third_party/unpeel/crates/unpeel-core/src/worktrees.rs`: the ownership
  predicate of `remove` extracted as `pub fn is_managed`, semantics unchanged.
- `crates/workers-unpeel/src/lib.rs`: the ownership gate in `remove_worktree`,
  the deleted rollback in `create_worktree_and_launch`, the shared
  `is_plain_group` predicate, and `WorkersProject::change_request_branch`.
- `crates/ui/src/workers/{model.rs,project_menu.rs,workspace.rs}` and
  `crates/ui/src/change_requests.rs`: remove dispatch, menu verb and confirm
  label keyed on `is_group`; both halves of the badge on
  `change_request_branch`.
- Out of scope: a worktree whose registry record has a branch but whose FOLDER
  is gone (17 of 46 records here) stays unremovable — that is
  `guard-worktree-deletion`'s territory. `sort_order` and `folder_color_id`
  still refuse an adopted worktree, since its parent is projected, not
  persisted. The menu still says "Remove worktree?" for an adopted checkout
  that is only de-registered; distinguishing it costs a wire field or a
  `canonicalize` per row per frame.
