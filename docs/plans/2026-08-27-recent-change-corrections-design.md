# Recent Change Corrections Design

## Scope

Correct the nine material findings found in the local `origin/main...HEAD`
review without expanding the product surface. The affected capabilities are
the active OpenSpec changes `add-projects-settings-page` and
`add-chat-transcript-export`, plus the repository-local Unpeel vendoring
contract. All commits remain local; no push, deploy, release, or tag is part of
this work.

## Decisions

### D1. Repair the current seams

Keep the existing `project_ledger`, `worktree_config`, `ProjectsPage`,
`ExportDoc`, and vendored `third_party/unpeel` boundaries. A page rewrite or a
new persistence layer would increase upstream-merge cost without fixing a
separate problem.

### D2. A ledger row is a filesystem project, never an organizational group

`projects_with_ledger` filters `WorkersProject::is_group` before reconciliation.
Worktrees remain eligible because they have distinct filesystem paths. The
pure reconciliation layer also rejects duplicate live paths defensively so one
path cannot create duplicate ledger entries or ambiguous UI element ids.

### D3. Projects configuration becomes an actual editor

The Config card selects `.comet/worktree.json` or an already-existing Cursor
config. The Worktree card edits shared, Unix, and Windows command lists and
saves only a changed normalized draft. The list pane scrolls. Project icons use
a collision-safe digest-derived filename, render the stored image when it can
be loaded, and remove the managed file during reset or forget. Repository links
preserve the parsed remote host instead of forcing `github.com`.

### D4. A failed setup leaves the worktree but blocks automatic launch

Creating a worktree remains successful even when its setup fails, preserving
the checkout for inspection. The result carries the failing command and reason
through the client and UI. `create_worktree_and_launch` does not launch a
Worker after setup failure. The setup runner drains bounded stderr while the
child runs and enforces timeout over the spawned process group so neither a
full pipe nor a descendant can escape the deadline.

### D5. Every export format consumes one sanitized projection

`ExportDoc` stores export-specific messages and parts, not raw
`SessionMessageEntry` values. Text and tool summaries are projected once;
reasoning, inputs, workflow state, inline output, diffs, and sidecar references
do not enter any renderer. Markdown, Text, and JSON serialize the same projected
parts and artifact index.

### D6. Vendoring provenance must be truthful, not reconstructed

The unavailable original patch for the sixteen pre-vendoring working-tree
changes cannot be recreated honestly. `third_party/AGENTS.md` will describe
ordinary vendored files, and `unpeel-upstream.toml` will record the known base
revision, the checked-in subtree tree id, the fact that the working tree carried
sixteen local changes, and that the original patch is unavailable. The checked-in
tree is reproducible; the historical limitation remains explicit.

## Error Handling

- Project config and icon operations report failures through the existing page
  error strip and never claim success after a partial write.
- Managed icon cleanup is limited to the app-owned icon directory and the exact
  path recorded for the selected project.
- Setup failures retain the worktree, name the command and reason, and never
  continue into worker launch.
- Export JSON cannot learn fields unavailable to Markdown/Text because the raw
  transcript types are absent from the renderer boundary.

## Verification

Each behavior starts with a focused failing regression test, followed by the
minimal implementation and the same test green. Work proceeds in batches of at
most three findings and allows at most three fix attempts per failure.

Focused gates:

- `cargo test -p zeron-workers-unpeel project_ledger`
- `cargo test -p zeron-workers-unpeel worktree_config`
- `cargo test -p zeron-ui projects`
- `cargo test -p zeron-ui chat_export`

Closeout gates:

- `cargo test -p zeron-workers-unpeel`
- `cargo test -p zeron-ui`
- `cargo fmt --all --check`
- strict validation and archive for both OpenSpec changes
- native visual QA for Projects list scrolling, config editing, custom icons,
  repository links, setup failure feedback, and all six export actions

## Local Commit Boundaries

1. Design and implementation plan.
2. Projects ledger and settings contracts.
3. Worktree setup execution and failure propagation.
4. Sanitized Chat Transcript Export.
5. Unpeel provenance and DOX alignment.
6. OpenSpec archive closeout after all gates.
