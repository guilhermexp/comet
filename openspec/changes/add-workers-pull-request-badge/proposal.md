# Change: The Workers sidebar names the pull request behind a worktree branch

## Why

A worktree row now names its branch (`adopt-external-worktrees`), and a branch
whose whole reason to exist is a change under review is half a fact: it says
`fix/correios-review-findings` and stays silent on whether that work is open,
merged or closed. The user asked for the PR tag next to it.

Every piece already exists, on the Orchestrator side:

- `change_requests::pull_request_badge` draws `#N` with the state tone
  (Open = success, Merged = code_text, Closed = danger) and a tooltip carrying
  the title; `ChangeRequestBadgeSurface::Sidebar` is its 16px variant.
- `AppState` owns the resolution: `ChangeRequestClientState` stores one
  `CheckoutChangeRequestStatus` per `ChangeRequestWatchKey`, and
  `reconcile_change_request_watches` opens a `WATCH_CHECKOUT_CHANGE_REQUEST`
  subscription per target, retiring the ones that left the set.
- The engine caches by `(checkout_id, branch, upstream_ref, remote)` under a
  demand lease: 2min TTL with a PR, 45s without, backoff to 15min on failure.
- `UiSettings::sidebar_show_pull_request` already gates the whole machinery
  through `set_change_requests_visible`.

What the Orchestrator has and Workers lacks is only the SET of checkouts to
watch. Its targets come from chats (`desired_watch_targets` reads each chat's
`source_context`); a Workers worktree is not a chat and never enters that set.

So this change adds a second source of targets to the existing reconcile
instead of building a second watcher. `crates/ui` already depends on
`zeron-engine` and `zeron-proto`, and `WorkersModel` already holds
`Entity<AppState>` — there is no new dependency and no new transport.

## What Changes

- `AppState` accepts a second set of watch targets alongside the chat-derived
  ones. They are unioned in `reconcile_change_request_watches`, so retention,
  the per-device unsupported cache, the visibility gate and task lifetime are
  the ones already in service.
- `WorkersModel` publishes one target per worktree project (a project carrying
  a `worktree_branch`) after each snapshot. Ordinary projects are NOT watched:
  a repository's default branch has no PR to name, and 46 registered projects
  would be 46 subscriptions for a badge nothing would draw.
- The Workers project row draws the badge beside the branch chip it already
  renders, on the same `sidebar_show_pull_request` toggle as the Orchestrator.

## Capabilities

### New Capabilities

- `workers-pull-requests`: which Workers checkouts get PR resolution, and how
  the sidebar row names the result.

### Modified Capabilities

None. The chat-derived targets, the badge and the engine cache are untouched.

## Impact

- `crates/ui/src/state.rs`: a second target set and its setter; one union in
  the reconcile.
- `crates/ui/src/change_requests.rs`: the lookup by `(cwd, branch)` a Workers
  row needs — the chat lookup re-verifies a chat's device/space identity, which
  a device-local project does not have.
- `crates/ui/src/workers/model.rs`: derive the targets from the snapshot.
- `crates/ui/src/workers/workspace.rs`: the badge on the project row.
- Out of scope: PRs for ordinary (non-worktree) projects, and a Workers-only
  toggle. Both are additive later if the shared toggle proves wrong.
