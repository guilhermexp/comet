# Tasks

## 1. Projection

- [x] 1.1 Add `detached: bool` to `GitCheckout` and set it on the arm that
      already builds the 7-char sha, in
      `third_party/unpeel/crates/unpeel-core/src/controller_host.rs`.
      `branch` keeps carrying the sha: `gitBranch` says what HEAD says.
- [x] 1.2 Hoist one verdict per project above the object build in
      `DiskCatalog::capture`: `is_group` from the canonical registry predicate,
      and the effective worktree branch as registry-first, disk second, gated
      on `!is_group` and `!detached`. Name the three clauses in a comment.
- [x] 1.3 Emit `worktreeBranch` and `isGroup` from those locals instead of the
      two independent predicates. `parentProjectID` is untouched.
- [x] 1.4 Carry the verdict into `HostCreateProject`: `is_folder: is_group`,
      the worktree path where a branch was resolved, and that branch.
- [x] 1.5 Extend `git_head_branch_reads_checkout_worktree_and_detached_head`
      for the new field, asserting the whole `GitCheckout` in the detached case.

## 2. Ownership

- [x] 2.1 Extract the ownership predicate of `worktrees::remove` as
      `pub fn is_managed(path: &Path) -> bool`, semantics unchanged, and call it
      from `remove`.
- [x] 2.2 In `LocalWorkersClient::remove_worktree`, drop the registry-identity
      early return and gate the disk removal on the registry's
      `worktree_branch` AND `is_managed`. Comment the field's new meaning:
      ownership, not identity.
- [x] 2.3 Delete the rollback `remove_worktree` from the failed-launch arm of
      `create_worktree_and_launch`.

## 3. One predicate for group

- [x] 3.1 Add a private `is_plain_group` in `crates/workers-unpeel/src/lib.rs`
      and use it in `remove_group` and in the rename branch of
      `set_project_organization`, which routes on the group predicate instead of
      its complement.

## 4. Badge

- [x] 4.1 Add `WorkersProject::change_request_branch`: gated by
      `worktree_branch`, valued by `git_branch`.
- [x] 4.2 Move BOTH halves onto it — `change_request_for` in
      `crates/ui/src/workers/model.rs` and `workers_change_request_targets` in
      `crates/ui/src/change_requests.rs` — so the subscription and the read name
      the same branch.

## 5. Removal verbs

- [x] 5.1 Dispatch the group removal on `is_group`, not on the presence of a
      parent, in `WorkersModel::remove_project`.
- [x] 5.2 Offer `RemoveGroup` on `is_group` in `project_menu_items`; `is_child`
      keeps governing Rename, FolderColor and NewGroup.
- [x] 5.3 Draw "Remove group?" on `is_group` in `render_project`, so the label
      matches the dispatch.

## 6. Verification

- [x] 6.1 `launching_into_a_created_worktree_is_not_rejected_as_a_folder` in
      `crates/workers-unpeel/tests/project_actions.rs`: a launch probe with no
      preset reaches "unknown preset id" instead of "project is a folder" or
      "worktree does not belong to project".
- [ ] 6.2 Extend `worktree_registered_as_a_plain_project_is_projected_as_a_worktree`:
      the same probe on the adopted fixture, and `remove_worktree` leaving the
      checkout on disk while the project leaves `bootstrap`. Until this lands,
      two Scenarios in the delta carry `Test: none` — "An adopted worktree
      accepts a launch" and "An adopted worktree is de-registered, not deleted".
- [x] 6.3 `a_group_inside_a_worktree_stays_a_group`: a group record sharing the
      worktree's path is a group and names no branch.
- [x] 6.4 `a_detached_worktree_names_no_branch_but_keeps_its_place`: no worktree
      branch, git branch is the short sha, not a group.
- [x] 6.5 `a_failed_launch_keeps_the_worktree_it_just_created` and
      `renaming_an_adopted_worktree_writes_the_project_record`, with a sibling
      case proving a plain group still routes to the group rename.
- [x] 6.6 The menu item is covered by `adopted_worktree_without_a_branch_removes_as_a_project`
      in `crates/ui/src/workers/project_menu.rs` and the route it dispatches by
      `a_child_that_is_not_a_group_removes_as_a_project`; the disagreeing-branch
      case is in `workers_change_request_targets_cover_worktrees_only`.
- [ ] 6.7 `cargo test -p unpeel-core --lib`, `cargo test -p zeron-workers-unpeel`,
      `cargo test -p zeron-ui`, `cargo fmt --all --check`.
- [ ] 6.8 Visual check on `scripts/dev-demo.sh` — the only UI evidence this repo
      has: "New worktree…" then launch into the new row starts a session in the
      worktree; a worktree adopted from a terminal launches, renames and removes
      with its folder intact; a group made on a worktree row shows no branch
      chip and offers "Remove group?".

## 7. Closeout

- [ ] 7.1 DOX pass in `crates/workers-unpeel/AGENTS.md`: the verdict is computed
      once in `DiskCatalog::capture` and feeds both the wire and the creation
      catalog; `is_folder` is an inert hint on this route; disk detection is
      gated on `!is_group` and refuses a detached HEAD; `worktree_branch` is an
      ownership record, and deleting from disk needs it plus
      `worktrees::is_managed`.
- [x] 7.2 DOX pass in `crates/ui/AGENTS.md`: the badge is gated by
      `worktree_branch` and valued by `git_branch`, in both halves, through
      `WorkersProject::change_request_branch`.
- [ ] 7.3 Update `third_party/unpeel-upstream.toml` in the same commit as the
      vendored edits.
- [ ] 7.4 Archive once 6.8 is confirmed, and only AFTER
      `distinguish-worktrees-from-groups` and `adopt-external-worktrees`: both
      write `## ADDED Requirements` to this same capability, and the two
      `## MODIFIED Requirements` here replace theirs. Archiving this change first
      leaves the spec holding an unqualified "report the checked-out branch as
      the project's worktree branch" next to the detached-HEAD carve-out that
      contradicts it.
