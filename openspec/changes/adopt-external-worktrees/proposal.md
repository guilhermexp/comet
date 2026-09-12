# Change: A worktree registered from outside the app is still a worktree

## Why

`create_worktree` records `worktree_branch` and `parent_id` in the registry, so
a worktree the app itself created reaches the sidebar nested under its parent
with the branch glyph. A worktree registered any OTHER way carries neither
field: `git worktree add` in a terminal, then "Add project…" on the resulting
folder, writes a plain project record.

That is the common case here, not the corner. Measured on this machine's
`~/.unpeel/app-state.json`: 46 registered projects, 8 of them live linked
worktrees on disk, and **zero** carry `worktree_branch`. Every one of the 8
reaches the sidebar as a root project with a folder icon and no branch — four
of them (`jk-wt-correios`, `jk-wt-assets`, `add-flat-theme`, an `.orchestrator`
checkout) sitting as siblings of the very project they are a checkout of.

The registry cannot know: it records what the app was told at registration.
Disk knows, unambiguously — git writes `.git` as a FILE, holding a `gitdir:`
pointer, only inside a linked worktree; an ordinary checkout has `.git` as a
directory. The projection already opens that exact file to emit `gitBranch`,
and already parses the `gitdir:` pointer to follow it. The pointer spells
`<main-repo>/.git/worktrees/<name>`, so the repository the checkout belongs to
falls out of the path the projection has already read.

## What Changes

- The `comet-local` projection detects a linked worktree from disk and fills
  the two fields the registry left empty: `worktreeBranch` (so the row draws
  the branch glyph) and, when the main repository is itself a registered
  project, `parentProjectID` (so the row nests under it).
- Registry values always win: detection only fills what is absent, so a
  worktree the app created is projected exactly as before.
- A worktree whose main repository is NOT registered keeps its root position —
  there is no parent row to hang it from — but still names its branch.

## Capabilities

### Modified Capabilities

- `workers-project-identity`: a worktree is recognised by its checkout, not
  only by its registration.

## Impact

- `third_party/unpeel/crates/unpeel-core/src/controller_host.rs`: `git_head_branch`
  becomes `git_checkout`, returning the main repository alongside the branch;
  two fills in the snapshot projection.
- No UI edit. `workspace.rs` already draws the branch glyph from
  `worktree_branch` and already orders the tree depth-first from
  `parent_project_id`; both start receiving values they never got on this route.
- Out of scope: worktrees whose folder no longer exists on disk (17 of the 46
  records here). Disk is the only evidence this change has, and they leave
  none.
