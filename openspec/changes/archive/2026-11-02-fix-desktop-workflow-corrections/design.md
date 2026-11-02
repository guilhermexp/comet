# Design

Context: `proposal.md`; source map and diagnostics supplied with the task (`worktree-diagnosis.json`, `omp-update-diagnosis.json`). Refinement of existing surfaces, no new runtime, dependency or wire field.

## W1 — causes found
1. `settings/worktrees.rs` flipped an in-memory `enabled` and saved only if a folder was already remembered, so the switch showed "on / Choose a folder" with nothing persisted; reopening showed off. A failed save also left the optimistic flip in place.
2. Worker checkouts resolved their root with `worktree_ownership::common_worktrees_root()`, which read only `ZERON_WORKTREES_DIR` and `~/.zeron/worktrees` and ignored the saved device preference that Chat worktrees (engine `WorktreePreferences`) honor. That is a concrete path to "agents create worktrees in unexpected places" even with a saved folder.

Decision: the switch always renders the persisted state; toggling either saves (`use=true` with the remembered folder / `use=false` keeping it) or, for a first enable, opens the folder choice and saves only the chosen, engine-validated folder. `common_worktrees_root()` applies the same precedence as the engine: env override → saved enabled absolute folder (`{data_dir}/worktree-settings.json`, data dir by the app's rule) → default. Malformed/disabled/relative preferences fall back to the default. Nothing is moved or deleted.

## W2
`activity_menu::project_activity_menu` sorted working rows by `updated_at` (and other sections followed snapshot order, itself activity-ordered). Each section now sorts by `(created_at, id)`. Native tags are rebuilt with the rows, so a tag still resolves to the row's Worker.

## C1
The staged strip was a sibling above the pill. It now renders inside a chrome wrapper that owns the border/fill/corners; the input frame keeps its explicit morph height (minus the moved border), so the chrome's natural layout reserves the real card height (variable-width text cards included). Appshots keep their separate surface; popups unchanged.

## B1
`set_right_active` closes Details (existing close sequence: tween, save, notify; width kept) when the Browser becomes the active surface — creation, new tab, navigation, tab selection, fallback. Re-selecting the already active Browser does nothing, so a manual reopen holds. File previews never close Details.

## B2
Details Servers rows emitted `cx.open_url("http://127.0.0.1:<port>")`. They now emit `OpenServer { chat_id, port }`; the Shell opens it on its next render (window needed): stale Chat → nothing; local host → `activate_session_link` (internal by default, explicit external preference respected); remote host → internal Browser that opens the port's preview route as soon as discovery lists it (never the remote host's loopback on this Mac, never a silent external launch). Targeted upstream comparison: `.tmp/evidence/desktop-corrections/B2-upstream-comparison.md` (upstream `d5a01522` has the same Chat-link router and no Servers section).

## U1
Concrete gap: `changed_binaries()` skipped any harness in the cancellation map, so an external update during WaitingForIdle was only noticed after the exclusive lease. Now a waiting update owns a `Notify`; the watcher signals it on a binary change; the update re-reads the version and, if already at/past the target, drops its queued lease request, reports Current and releases the pending marker. Cancellation and lease acquisition race through one `select!` (biased: cancel, lease, external), so exactly one outcome wins; active runs never lose their lease. Timeout keeps the redacted tail of what the updater printed and names the timeout; failure output is redacted. Timeout policy, retries, `omp update` plans and 64 MiB rpc diagnostics are unchanged.

## Not proven remotely
Native macOS menu bar, gpui compositor appearance, WebKit preview routing, and an actual OMP installer run are left to Mac acceptance (see DELIVERY checklist).
