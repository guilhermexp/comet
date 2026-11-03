# Design

## Context

See `proposal.md` for intent and the delta spec for acceptance. Base mapped: `122c3090f`.

- `details_sidebar/view.rs:199-221,3143-3151` derives Projects worked only from the selected `AppState.transcript`. The transcript LRU (`state.rs:43-54`) holds only 12 previously opened Chats / 64 MiB. Neither is a correct source for every sidebar row.
- `details_sidebar/worked_projects.rs:54-323` already defines supported tools, leaf roots, own-checkout exclusion, component boundaries and first-contact order. `worked_project_label` resolves registered identity for the Details presentation.
- `engine/skill_usage.rs` demonstrates sequential cold-history reads and history/session/registry invalidation. `DocHost::read_local_doc_with_coverage` reads a retained snapshot without `open()`, room join or resident handle; `watch_history_changes` supplies invalidation.
- `workers-unpeel::space_links::{workers_state_path, linked_checkouts_at}` supplies persisted checkout→Space links. They must be filtered to the owning device. The parent-directory watcher must handle atomic app-state replacement.
- `Shell::render_chat_row` and `sidebar_chat_data` are shared by normal and archived rows. Normal metadata is the line containing Branch/PR. Compact is 29 px and deliberately does not render that metadata line.
- `edge/src/registry-core.ts:24-37,55-85,147-160` already preserves nested optional fields and merges them per field; no Chat-field allowlist or session-doc materializer change is needed. Keep the existing 16 KiB operation budget and use a field-only update.

## Goals / Non-Goals

**Goals:** correct attribution for unselected Chats, root deduplication, no history work in render, and the existing metadata/Show UX.

**Non-Goals:** changing the meaning of Details, deriving projects from titles or launch targets, adding trees/counters/actions, changing Compact, loading every archived transcript at startup, or modifying providers/installed apps. No new dependency.

## Decisions

### D1 — Compact host-produced summary, consumed through existing registry watches

Add optional `Chat.worked_projects: Option<ChatWorkedProjects>` (wire `workedProjects`). `ChatWorkedProjects` holds `project_ids: Vec<String>` in first-contact order and `complete: bool`; names and raw filesystem paths are not copied into the summary. Absent metadata is unknown, not confirmed empty. Resolve current names from the synced Space registry on the viewing device.

Only the current host produces a Chat's attribution using its own associated-checkout catalog and home context. Publish a field-only registry update only when the summary changes; do not touch `updatedAt`, source context, selection or Session execution state. Preserve the field through normal registry/workspace round-trips and unrelated Chat edits. Keep old readers and rows valid through an optional/defaulted field.

### D2 — Share the existing pure path rule; retain presentation adapters

Move the tool/path matching core into `zeron_proto::worked_projects`, accepting borrowed roots and an iterator of `&ToolCall`, returning matched root indices in chronological order. The engine/UI adapters filter assistant Tool parts; doc/UI/workers types stay out of proto. Keep existing Details output and tests, including own-checkout and leaf-root rules. UI-specific branch icons/names stay in the current adapter.

The host maps matched roots to explicit local `space_id` values and deduplicates by ID, not display name, repository ID, branch or basename. No fork/child or independent Worker transcript is added merely through its parent relationship; existing attribution semantics remain the source of truth.

### D3 — One cancellable engine service, not one transcript watch per row

`WorkedProjectsSync`, owned by `EngineCore`, follows `SpacesSync` start/shutdown ownership. Attach history, Chats/Spaces and Session invalidation before backfill; process one cold snapshot at a time in `spawn_blocking`, discard it after computing the contribution, and publish only from the live supervisor after await. On lag, rebuild the affected scope. Self-authored summary updates must not generate a rescan loop.

Backfill local nonarchived Chats outside startup's UI/render path. Read errors or partial history preserve known project IDs instead of replacing them with confirmed empty. A complete authoritative read can replace the summary exactly. Recheck ownership/context before publishing so a stale result cannot be assigned to another host or cwd.

Use the existing `FsWatchHub` for the parent of `workers_state_path()`, with callback-to-kick only. Compare normalized path/Space pairs before invalidating; preset/pin edits must not rescan history. Follow the existing SpacesSync background repair contract (120 s) for missed events, missing directories or failed native watcher attachment. No render polling.

To respect the existing prohibition on loading unviewed archived Chats, add a small batched `EnsureChatWorkedProjects` request (`chatIds`) for displayed rows whose summary is still absent. Validate that all requested Chats belong to the target host, deduplicate them and enqueue background calculation; results arrive via existing WatchChats, not a new transcript stream. UI coalesces once per request/connection generation outside render, with no redraw retry loop. Hidden labels request no hydration. Offline/old hosts retain already-known metadata and do not fabricate labels.

### D4 — Preserve the existing sidebar metadata row and Compact policy

`ActiveChatRow` carries root labels derived from that Chat's summary. Extend the normal Branch/PR line with the current folder icon and 11/14 px label style, separated within its existing horizontal flex budget. Use the existing single-line/faded-label treatment and a tooltip for complete names. Clicking a label continues selecting the Chat through the row; there is no new navigation action.

Metadata visibility/height considers labels even when Branch is off or absent. Keep every height consumer consistent: painted row, pinned slots, drag/transfer, disclosure/FLIP and archived slots. Preserve PR and time/status affordances. Compact continues hiding this metadata line; it does not gain a new row or insert labels into the title. Command-palette callers remain unchanged in UX and pass no worked-label presentation unless they are the actual Sessions sidebar.

### D5 — Use the existing settings pipeline, enabled by default

Add `sidebar_show_worked_projects: bool` (JSON `sidebarShowWorkedProjects`, default true) to UiSettings, its central writer/merge and `apply_shell_settings`. Add `ShowWorkedProjects` immediately after Branch in the existing Show enum/rows/labels/icons/selected arrays. Toggle via the current action and `schedule_save`, keeping the submenu open. Do not tie it to Branch, Location or Details and do not replace the entire settings file from a stale copy.

## Approaches considered

- **Selected transcript or UI LRU only:** cheapest patch, rejected because unselected/never-opened Chats would be wrong or empty and restarting would lose attribution.
- **Open WatchDocMessages for every row:** rejected because it opens/pins docs, requests synchronization and serializes full history merely to draw compact labels.
- **Host summary + existing WatchChats/WatchSpaces (chosen):** modest additive metadata and one background producer; preserves attribution across restart/offline viewing without resident transcripts. A single batched demand request covers visible legacy archived rows without violating startup bounds.

## Risks / Trade-offs

- Initial historical calculation costs one scan of the retained snapshots → sequential, outside render, reuse existing invalidation and never pin all docs.
- Partial/corrupt/missing snapshots → preserve known IDs, mark incomplete and recalculate on genuine history changes rather than claiming empty.
- Same path/branch on different devices → host ownership and explicit Space links are mandatory; never use the viewer's local catalog for a foreign Chat.
- Metadata update accidentally changes sidebar ordering or overwrites neighboring fields → field-only setter plus ordering/concurrent-field regression tests.
- Long labels or metadata without Branch break geometry → test production row bounds, all height consumers and PR click behavior, then native narrow/wide smoke.

## Migration Plan

Implement in one isolated Worker/worktree based on the current local main; preserve the other sessions' changes. New metadata is optional; no transcript rewrite, container rename or destructive migration. Updated hosts backfill available history; old hosts remain usable with unknown labels. Rollback can ignore the optional metadata and restore only presentation/settings code without deleting history.

Delivery remains local: focused tests, native isolated smoke, final `review-pair.sh --gate` with the correct base and full manual contingency if unavailable, then local integration, OpenSpec archive, sync and cleanup of only task-owned resources. No push, release, tag or installed-app replacement is authorized by this change.
