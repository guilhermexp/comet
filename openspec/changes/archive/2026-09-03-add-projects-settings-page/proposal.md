# Change: Projects settings page (parity with orchestrator.dev)

## Why

comet knows a project only while it is in use. The workers sidebar groups live
sessions by project, and "Remove project" in its context menu calls
`WorkersClient::remove_project`, which deletes the record from
`~/.unpeel/app-state.json` **and** every session under it
(`crates/workers-unpeel/src/lib.rs:1774`). Nothing survives. There is no place
that answers "what have I worked on in this app", no place that holds a
project's path, icon or first-seen date, and no place to configure what runs
after a worktree is created.

The reference implementation is orchestrator.dev's **Settings → Projects**,
read from its shipped bundle (`app.asar` →
`src/renderer/components/dialogs/settings-tabs/agents-project-worktree-tab.tsx`
and `src/main/lib/trpc/routers/projects.ts`). Its two panes — a searchable
project list and a five-card detail — are the target, and its most important
property is one the screenshots make plain: its sidebar shows 5 project groups
(the ones with live sessions) while Settings → Projects shows 11 rows. The two
lists are decoupled on purpose. The settings list is the durable ledger of
every project ever given entry to the app.

comet is well placed for the rest: the project registry is already local
(`ControllerHostRuntime::owner_transport("comet-local")` — the `bootstrap` HTTP
call never leaves the process), the native folder picker is already used for
Add Project (`workers/workspace.rs:160`), and `parse_git_remote`
(`crates/engine/src/source_control.rs:372`) already turns a remote URL into
host/owner/repository.

## Decisions

- **D-01: Two registries, not one.** The working set
  (`app-state.json → projects[]`) is what the sidebar shows and what
  `remove_project` prunes. The settings page reads a **ledger** that only ever
  accumulates. A project appears in the ledger the first time it is seen and
  stays after it leaves the working set. The ledger is the superset; the
  working set decides which rows are *live*.
- **D-02: The ledger is `comet_projects` in `app-state.json`.** That file
  already carries comet-namespaced keys (`comet_worker_parent_notifications`,
  `comet_workers_appearance`, `comet_workers_preset_catalog_version`), its
  module refuses to drop a top-level key it does not model, and it holds a
  flock across every read-modify-write
  (`third_party/unpeel/crates/unpeel-core/src/app_state.rs`). The ledger
  survives `remove_project` by construction: `remove_project_record` touches
  only `projects`, `project_folder_colors` and `session_sort_modes`.
  *Rejected:* a new file beside `ui-settings.json` — that file is device-local
  UI chrome (pane widths, tab order), and a second store means two places that
  can disagree about the same project.
- **D-03: The ledger is keyed by canonical path, not by project id.** Ids are
  minted fresh on every `add_project` (`comet-<uuid>`), so removing and
  re-adding the same folder would orphan its history. The reference keys on
  path too (`projects_path_unique`). Path is what survives.
- **D-04: The ledger holds only what cannot be recomputed** — `path`, `name`,
  `added_at_unix_ms`, `last_seen_at_unix_ms`, `icon_path`. Git state, anchor
  commits, archived-session counts and the live last-activity are read fresh,
  exactly as the reference computes `gitStatus`, `commitContext` and
  `lastActivityAt` instead of storing them.
- **D-05: "Last opened" has two sources.** A live project derives it from
  `max(session.updated_at_unix_ms)` over the bootstrap sessions — the same
  derivation the reference runs over its `chats` table. A ledger-only project
  shows the `last_seen_at_unix_ms` frozen when it left the working set,
  because its sessions were deleted with it. The row is never blank for a
  project that was really used.
- **D-06: Worktree setup is a file in the checkout, and it gets an executor.**
  Config lives at `.comet/worktree.json`, with `.cursor/worktrees.json`
  detected and offered when it already exists, mirroring the reference's
  detection order. Writing that file is only worth doing if something reads
  it: comet creates worktrees (`unpeel_core::worktrees::create`) and runs
  nothing afterwards — there is no `setup-worktree` reader anywhere in the
  repo — so this change also runs the commands after `create_worktree`, with
  `ROOT_WORKTREE_PATH` in the environment and a per-command timeout.
- **D-07: "Fill with AI" and "Run Auto Doc" launch a worker, not a Chat.**
  These projects are the worker registry, and
  `WorkersLaunchRequest.initial_text` already carries a prompt into a new
  session in a given project. A comet Chat belongs to a Space and would put
  the work in a different tree from the project the button was pressed on.
- **D-08: Auto Doc ships only its Run half.** The reference's second row
  creates a local automation triggered by `pr_merged` on GitHub. comet has no
  automation store and no webhook intake, so that row is omitted rather than
  rendered dead. Named here so the gap is a decision, not an oversight.
- **D-09: The page's remove means "forget", not "remove from the sidebar".**
  The sidebar's existing "Remove project" drops the working-set entry and its
  sessions. The Danger Zone action clears the **ledger** row and the stored
  icon and touches no session. Two verbs, two surfaces, each labelled for what
  it does — and the dialog says which one the user is about to get.
- **D-10: Git runs locally from the client crate.** `reveal_project` already
  shells out with `std::process::Command` inside `zeron-workers-unpeel`, and
  the UI calls it through `WorkersModel::run_unit_action` on the background
  executor. `git rev-parse`, `git remote get-url`, `git log -1 --until`,
  `git init` and `gh repo create` follow that same path. No new RPC and no
  engine round-trip: the registry is local by construction.
- **D-11: Git facts are computed, never taken from the wire.**
  `WorkersProject.git_branch` is deserialized from `gitBranch`
  (`lib.rs:316`) but `controller_host.rs` never emits it — only the vendored
  TUI host does (`unpeel-tui/src/sessions.rs:1676`) — so it is always `null`
  through `comet-local`. Rather than patch vendored code, the page reads git
  from the project path directly, which it must do anyway for `is_repo`, the
  remote, owner/repo and the two anchor commits, none of which the wire
  carries.

## Non-goals

- Cloning from GitHub, locating an existing clone, or picking a clone
  destination (the reference's `cloneFromGitHub`, `locateAndAddProject`,
  `pickCloneDestination`). The sidebar's Add Project is the intake path.
- The Auto Doc **automation** row (D-08).
- Any change to comet Spaces, Chats or the synced session tree. This page is
  about the worker project registry only.
- A fixed "Home" pseudo-project.
- Patching `third_party/unpeel` (D-11).
