# Sidebar worked-project labels — Implementation Plan

> For agentic workers: REQUIRED SUB-SKILL: `/implement`, applying this change task-by-task with TDD. One isolated Worker owns the implementation; in-process subagents are read-only. Checkboxes are the execution ledger.

**Goal:** show root-project labels beside branch metadata for each Sessions row, configurable through the existing Show menu.

**Architecture:** a shared pure matcher feeds a host-owned compact Chat summary; WatchChats/WatchSpaces supply UI data. One background producer reads retained snapshots without opening all docs; a small batched demand request covers displayed archived rows with absent summaries.

**Tech Stack:** existing Rust/gpui/Loro/tokio/FsWatchHub; no new dependency.

**Spec:** `specs/chat-navigation-and-sidebar/spec.md`. **Design:** `design.md`, D1–D5.

## Global Constraints

- Read the native DOX chain before edits. Base mapped at `122c3090f`; isolate from other sessions and preserve their OpenSpec changes. Use references/LSP before exported type changes; migrate every affected literal/caller without shims.
- Follow `/Users/guilhermevarela/.agents/skills/impeccable/SKILL.md` for context/refinement and preserve the incumbent UI. No redesign or new compact row.
- No inferred project names/aliases, new dependencies, global heavy-run queue, wrapper reinstalls, provider configuration edits, push, release, tag or installed-app replacement. Native Cargo tools retain jobs=2 and sccache.
- Tests must cover consumer-visible behavior, not source text, wiring, copied fixtures or mock echoes. Keep history I/O and filesystem work off UI render and async executor threads.

## Review Focus

- A different, never-opened Chat must not receive the selected Chat's labels.
- Same-named branches and worktree folders must not override registered Space identity or duplicate the root.
- Partial history, old hosts, changed ownership and catalog reassociation must not erase known IDs or publish stale results.
- Summary updates must not move rows by touching `updatedAt`, overwrite adjacent metadata, or cause a rescan loop.
- Branch off, Compact, Archived and narrow rows must preserve heights, PR interactions and the persisted Show preference.

## 1. Shared attribution and additive persisted contract

**Files:** new `crates/proto/src/worked_projects.rs`; modify `proto/src/{lib,entities}.rs`, `doc/src/{workspace,registry}.rs`, `engine/src/workspace_host.rs`, `rpc/src/method.rs`; retain the presentation adapter in `ui/src/details_sidebar/worked_projects.rs`. Update only corresponding existing AGENTS contracts after proof.

**Interfaces:** `ChatWorkedProjects { project_ids: Vec<String>, complete: bool }`; optional `Chat.worked_projects` / wire `workedProjects`; `worked_root_indices<'a>(calls: impl IntoIterator<Item = &'a ToolCall>, roots: &[&str], own_checkout: &Path, home_dir: Option<&Path>) -> Vec<usize>`; `EnsureChatWorkedProjectsParams { chat_ids: Vec<String> }` / wire `chatIds`; method `ENSURE_CHAT_WORKED_PROJECTS`, acknowledgement `()`; field-only `WorkspaceHost::set_chat_worked_projects(chat_id, summary)` over the registry setter. Keep concrete error types consistent with existing setters.

- [x] 1.1 Add behavioral regressions for first-contact order, leaf roots, own-checkout exclusion, structured paths with spaces/punctuation, Exec boundaries and root-ID deduplication. Use literal expected `space-comet` once for several checkout roots and `space-bops` separately despite colliding branch names. Watch the missing behavior fail before implementation; preserve existing Details results. Verify with `cargo test --locked -p zeron-proto worked_projects` and `cargo test --locked -p zeron-ui --lib worked_project` after extraction.
- [x] 1.2 Implement the shared matcher and adapters, plus the optional metadata round-trip in both registry/workspace representations. Prove legacy rows decode unknown, partial differs from empty complete, unrelated Chat edits preserve the field, and setting it preserves title/source context/updatedAt. Verify with focused `cargo test --locked -p zeron-doc worked_projects`; inspect the generic registry/edge materialization contract and change/test a consumer only if required for the optional field. Do not change session-doc containers. Update the owning proto/doc DOX contracts once these behaviors pass.

## 2. Host-owned background summaries

**Files:** new `crates/engine/src/worked_projects.rs` and `crates/engine/tests/worked_projects.rs`; modify `engine/src/{lib,rpc,workspace_host}.rs` as required by the new service and request; use existing `doc_host.rs`, `skill_usage.rs`, `spaces.rs` and `fs_watch.rs` patterns without refactoring those unrelated services.

**Consumes:** section 1 matcher/summary/request and existing `space_links::linked_checkouts_at`, `DocHost::read_local_doc_with_coverage`, `watch_history_changes`, workspace/session watches.

**Produces:** `WorkedProjectsSync::start(doc_host, workspace, sessions, device_id) -> Self`, `ensure(&self, chat_ids: &[String]) -> Result<(), EngineError>` and cancellable/awaited `shutdown(&self)`; EngineCore owns exactly one instance.

- [x] 2.1 Add integration cases `worked_projects_cold_unselected_chats`, `worked_projects_partial_history_preserves_known_ids`, `worked_projects_catalog_reassociation`, `worked_projects_foreign_host_rejected`, `worked_projects_archived_demand_is_scoped`, and `worked_projects_publication_preserves_chat_order`. Seed actual retained snapshots and associated checkout state in temporary directories; verify A→Comet and B→bops without opening either Chat, no doc handles/room joins, no cold scan of unseen archived Chats, and exact field-only publication. Watch failures before building the service; verify green with `cargo test --locked -p zeron-engine --test worked_projects`.
- [x] 2.2 Implement the service and typed request routing per D3. Subscribe before backfill, read sequentially in blocking work, recheck owner/context before publication, preserve known IDs on partial/error, suppress self-update loops, and handle history lag/cancelled shutdown without late registry writes. Watch the catalog parent through FsWatchHub and compare path/Space links so a real reassociation refreshes without another tool call; follow the existing 120-second background repair policy. A batched ensure rejects foreign/nonexistent Chat targets before enqueueing and does not open transcripts. Verify the section 2 integration cases, then document ownership/cancellation and the additive request in the existing engine/proto DOX contracts.

## 3. Sidebar labels and Show preference

**Files:** `ui/src/{settings,state,shell}.rs`, `ui/src/shell/{spaces,command_palette,session_hover_card}.rs` for actual callers; new focused `ui/src/shell/sidebar_worked_projects_tests.rs` if existing local tests cannot stay focused; extend `ui/examples/sidebar-fixture.rs` only for the necessary native proof. Update `ui/AGENTS.md` and `fork_changelog.md` after smoke proof.

**Consumes:** each Chat's own optional summary and current Space registry, never the selected transcript. **Produces:** root-name presentation passed through ActiveChatRow, `sidebar_show_worked_projects` / JSON `sidebarShowWorkedProjects` (default true), and `ShowWorkedProjects` after Branch in the existing Show menu. The `render_chat_row` extension receives explicit label presentation; all existing callers and height calculations are migrated, with no command-palette UX expansion.

- [x] 3.1 Add production GPUI scenarios under the `sidebar_worked_projects` filter. Assert distinct A/B rows; Comet dedup across worktrees; two real projects in order; no labels from title/source Space alone; live rename; Branch/Location off and branch absent; Projects worked off; Compact unchanged; active/archived geometry and PR click behavior; clipped long labels with complete tooltip. Verify RED→GREEN with `cargo test --locked -p zeron-ui --lib sidebar_worked_projects` and existing `sidebar_chat_height_tracks_visible_metadata` / section-drop / PR destination tests.
- [x] 3.2 Add the Show entry using the current menu action and central settings pipeline (`apply_shell_settings`, defaults, exhaustive merge, schedule_save). A real menu click must change only worked labels and keep Show open; flush/load/new-Shell must retain the preference; a stale unrelated settings save must not revert it or neighboring options. Verify through the `sidebar_worked_projects` filter, not by asserting source text or only a boolean.
- [x] 3.3 Wire per-Chat summaries and batched hydration outside render. Resolve root names by ID, retain first-contact order, handle missing/old/offline data conservatively and avoid a redraw request loop. Share the normal metadata width with Branch/PR using incumbent icons, typography, overflow and tooltip treatment. Update all painted/slot/pinned/drag/disclosure/archived height consumers together. Run the section 3 behavioral cases and existing sidebar geometry/interaction tests on the resulting tree.
- [x] 3.4 Exercise the actual native sidebar and Show submenu in isolated synthetic state: 280/310 px and wider; two unselected Chats with different projects, several Comet worktrees→one comet, normal/Compact, Branch off, labels off/on, long/multiple names, Archived and reload persistence. Avoid providers and the real Workers daemon; extend the fixture's isolated model path rather than accepting real daemon data as a seed. Capture the surface, fix observed defects in one batch, confirm once and remove temporary scaffolds. Record screenshots and limits under `.tmp/verify/add-sidebar-worked-project-labels/`; update only the existing UI DOX contract/changelog after proof.

## 4. Integrated verification and review

- [x] 4.1 On the final implementation tree, run the focused commands above plus affected existing settings/sidebar/Details regressions and format only changed Rust scopes. Record executed commands/results and the native proof; no claim of a full workspace suite unless actually run. Update task checkboxes only from accepted evidence and validate `openspec validate add-sidebar-worked-project-labels --strict`.
- [ ] 4.2 Before the final delivery commit, run `/Users/guilhermevarela/orchestrator.v5/scripts/review-pair.sh --repo <checkout> --base <implementation-base> --out <checkout>/.tmp/verify/add-sidebar-worked-project-labels/review-pair/1 --gate` on the full correct snapshot. Resolve findings. If quota/service/incomplete review prevents completion, immediately record full manual contingency with base/final tree or file hashes, examined files/contracts, findings/dispositions, functional evidence and a clean final secret scan in `manual-review/1/summary.md`; no blind retries, `--agent` or `--use-credits`. Deliver the local commit, changed files and actual review modality; do not publish.

## Workflow follow-up

- After implementation acceptance, integrate locally without absorbing other sessions' work; do not ask again for already-authorized actions. If the base moved, reconcile only overlapping changes and verify the combined affected paths.
- Archive this change through the OpenSpec CLI after its tracked tasks are complete; do not edit canonical specs by hand.
- Run sync on the affected existing AGENTS contracts, then cleanup only task-owned integrated branches/worktrees/builds/temporary services. Preserve receipts, shared caches, provider profiles and unrelated WIP. Report logical artifact size separately from observed free-space delta.
