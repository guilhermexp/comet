# Design: the ledger and the working set

## The problem in one screenshot pair

In orchestrator.dev the sidebar lists 5 project folders — the ones with visible
sessions — while Settings → Projects lists 11 rows, including projects with no
session at all and two worktrees. Its sidebar is session-derived; its settings
list is `SELECT * FROM projects`. Removing a project from view never removes
its metadata.

comet has only the session-derived half. `remove_project` deletes the record
and every session under it in one call, so "what have I worked on" has no
answer and a project's first-seen date is unrecoverable the moment it leaves
the sidebar.

## Where the two sets live

```
~/.unpeel/app-state.json
├── projects[]            ← working set. add_project appends, remove_project prunes.
│                           Read by DiskCatalog::capture → bootstrap → list_projects
│                           → the workers sidebar. 5 keys: id, name, path,
│                           sort_order, workspace_id.
└── comet_projects[]      ← NEW. The ledger. Appended on first sight, never pruned
                            by a sidebar removal. Keyed by canonical path.
```

Same file, same flock, same atomic rename, same "never drop a top-level key"
guarantee. `remove_project_record` enumerates the three keys it clears
(`projects`, `project_folder_colors`, `session_sort_modes`); `comet_projects`
is not among them, so survival is structural rather than a rule someone has to
remember.

## Reconciliation, once per bootstrap

The page renders `ledger ⟕ working_set`, joined on canonical path:

| In ledger | In working set | Row shows |
|---|---|---|
| yes | yes | live: name/path from the working set, `added_at` from the ledger, last-opened derived from sessions |
| yes | no | ledger-only: name/path/last-seen frozen in the ledger, no session actions |
| no | yes | first sight — the ledger row is created (path, name, `added_at = now`) and the row is live |

The third case is what backfills existing installs: the four projects already
in `app-state.json` gain ledger rows the first time the page (or any bootstrap)
runs, with `added_at` stamped at that moment. That date is honestly "first seen
by the ledger", not "first opened ever" — the information to do better does not
exist, and inventing an earlier date from session manifests would be a guess
dressed as a fact.

`last_seen_at_unix_ms` is refreshed on every reconciliation for live projects,
which is what makes a ledger-only row's frozen value meaningful: it is the last
moment the project was actually in the working set.

## What is derived, and from where

| Field | Source | Cost |
|---|---|---|
| Last opened (live) | `max(session.updated_at_unix_ms)` where `session.project_id` matches | free — sessions come in the same `bootstrap()` |
| Archived sessions | `project.archived_session_count` | free — same call |
| Worktree / group / parent | `worktree_branch`, `is_group`, `parent_project_id` | free — same call |
| Is a repo, remote, owner/repo | `git rev-parse` + `git remote get-url origin` + `parse_git_remote` | one process pair per selected project |
| Added / Last-opened commits | `git log -1 --until=<ISO> --format=%H%x1f%h%x1f%s%x1f%cI` | one process per anchor, selected project only |

Git runs only for the **selected** project, never for the whole list — the
reference does the same (`getGitStatus` and `getCommitContext` are per-id
queries, not part of `list`).

## Why not a new file

`ui-settings.json` (`crates/ui/src/settings.rs:52`) is device-local UI chrome:
pane widths, collapse flags, tab order. Its whole contract is "safe to lose".
A project ledger is not safe to lose, and putting it there would give comet two
stores that can disagree about the same project — one pruned by
`remove_project`, one not, with no lock between them. The shared file already
solved the locking and the forward-compatibility problem; using it is the
smaller change.

## Correction pass after implementation review

The working-set projection excludes organizational groups before it reaches
the ledger. Groups deliberately reuse their parent's filesystem path, while
the ledger key is the path; admitting both would create duplicate rows and an
ambiguous selection key. Worktrees remain because their paths are distinct.
`reconcile` also deduplicates live paths defensively.

The Config and Worktree cards are editors, not read-only summaries. Their
state consists of one selected supported target plus shared, Unix and Windows
command groups; normalization removes blank/comment lines, and unchanged
state does not write. The project list scrolls independently of the detail.

Project icon names use a fixed SHA-256 digest of the canonical project path.
Only files inside the app-owned icon directory are eligible for cleanup, and
reset/forget clear metadata and that exact file. Repository browser links
retain the parsed remote host.

Setup execution drains bounded stderr concurrently. On Unix the shell owns a
fresh process group so timeout can terminate descendants. A setup failure
keeps the created worktree but is carried as command plus reason through the
client; automatic launch stops before starting a Worker.
Process exit signals can interrupt the same process's hook-ingress accept loop;
`WouldBlock` and `Interrupted` are both transient and retry, while other accept
errors still retire the listener.
