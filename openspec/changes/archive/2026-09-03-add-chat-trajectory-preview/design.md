## Context

See `proposal.md` for motivation. Comet currently projects normalized `AgentEvent`s into two intentionally different stores:

- Run Journal is append-only local JSONL used for recovery and may contain complete sensitive values.
- Chat Transcript is synchronized product state that folds, bounds, or omits technical detail.

`SessionsEngine::publish` is the common ordered seam, but it is synchronous and therefore cannot tolerate a shared database lock or unbounded observability work. Existing right-pane surfaces own their GPUI entities and async watch tasks; `AppState` supplies engine access but should not retain state for a closed on-demand surface. Chat deletion can arrive through a local RPC, synchronized WorkspaceDoc change, or Space cascade, so cleanup cannot depend on one command handler.

The durable vocabulary and read-model boundary are recorded in `CONTEXT.md` and ADR 0004. The detailed implementation scaffold is `docs/plans/2026-09-01-1211-feat-chat-trajectory-preview-plan.md`.

## Goals / Non-Goals

**Goals:**

- Capture ordered semantic main Chat execution history continuously without affecting agent execution or recovery.
- Provide indexed multi-run history and coherent live updates from one device-local product contract.
- Preserve privacy by storing sanitized values and resolving raw data only through explicit owner-checked local access.
- Reproduce the confirmed three-lane timeline, hierarchical ledger, synchronized inspector, folding, search, timing, and live-scroll interaction in native GPUI.
- Reuse current titlebar and right-pane lifecycle ownership, including per-Chat deduplication and responsive takeover.
- Import eligible legacy journals without inventing timestamps, completion, or segmentation.

**Non-Goals:**

- CLI Worker trajectories.
- Cross-device Trajectory synchronization.
- Raw data export or persistent raw reveal state.
- Replacing Run Journal recovery or Chat Transcript projection.
- Age-, size-, or run-count-based pruning while a Chat exists.
- A new keyboard shortcut, pane owner, global Details integration, or Capture behavior change.

## Decisions

### 1. Engine-owned SQLite read model

Create a versioned profile-local Trajectory database using the repository's existing `rusqlite`, WAL, busy-timeout, transaction, and migration conventions. SQLite supports bounded paging, search, ranges, multi-run queries, source fingerprints, and deletion without turning JSONL into a product query API.

The database is neither synchronized nor embedded in SessionDoc. Run Journal remains authoritative for recovery; Trajectory is rebuildable observability state whose completeness is explicit.

Alternatives rejected:

- Query Run Journal directly for normal UI reads: couples presentation to recovery format, requires repeated full scans, and exposes raw bodies by default.
- Store Trajectory in Loro/SessionDoc: violates the device-local privacy and synchronization boundary in ADR 0004.
- Add another JSONL projection: inexpensive to append but unsuitable for indexed navigation and coherent history/live reads.

### 2. Bounded fail-open capture path

`SessionsEngine::publish` projects semantic records after Run Journal sequence identity exists. The synchronous publisher performs bounded projection and nonblocking enqueue only. One ordered writer task owns the SQLite writer connection and batches transactions; independent read connections serve RPC snapshots and legacy reads under WAL.

The capture queue is bounded. Queue saturation, migration failure, or transaction failure records or reports a degraded interval and never blocks/fails the agent run, Run Journal append, event publication, or Chat Transcript folding. Token-level text/reasoning chunks are coalesced into stable partial records on the existing presentation cadence; lifecycle edges, completed model/tool records, usage, status, errors, and final summaries remain durable semantic transitions.

A single `Mutex<Connection>` shared by writes and RPC reads is rejected because historical reads could stall synchronous publication.

### 3. Stable identity and schema

The shared proto contract defines stable identity independent of row indexes:

- Chat identity.
- Run identity.
- source sequence assigned by the journal/publication seam.
- semantic sub-record identity for partial/final replacement.
- optional parent tool-use and call identity for nested subagent events.

Records carry semantic kind, lane, hierarchy, status/error precedence, optional timing/usage/correlation fields, sanitized Payload/Result previews, schema metadata where bounded, and an opaque raw-source reference. Timing is a tagged recorded-or-sequence-only contract; absent data stays absent through storage, RPC, layout, and formatting.

Partial and final updates share stable identity so reconciliation replaces rather than appends duplicate rows.

### 4. Sanitized persistence and narrow raw resolution

SQLite stores only bounded product-safe representations and opaque source references. It does not duplicate raw prompts, file contents, tool inputs, or tool results.

The opaque reference includes Chat, source sequence, and nested parent/call identity when needed to disambiguate `AgentEvent::Subagent` wrappers. Raw reveal is a local-only unary RPC scoped to Chat, record, and field. The engine verifies Chat ownership and every reference dimension, resolves the specific Run Journal event through a versioned adapter, and returns unavailable on absence, mismatch, deletion, corruption, or unsupported source version.

Sequential JSONL parsing runs in `tokio::task::spawn_blocking` with line-size guards equivalent to bounded tool-input lookup. Trajectory methods are excluded from the forwardable device/edge method set. The response is never cached globally: only the open `TrajectoryView` holds one revealed value and clears it on record change, close, profile change, or Chat deletion.

### 5. Snapshot-watermark-delta protocol

The watch RPC establishes subscription ordering before or atomically with the historical query. It returns bounded snapshot/page frames with a last-sequence watermark, then ordered deltas strictly after that watermark.

The client reconciles by stable record identity. Duplicate delivery is idempotent; a noncontiguous delta causes an explicit regap/resnapshot path instead of silent sorting. Dropping the surface cancels only its watch, not engine capture. Deleting the Chat closes the stream with a typed terminal state; store faults produce a typed degraded snapshot interval.

Each `TrajectoryView` owns one watch task and mutable snapshot. `AppState` is only the engine connection source, preserving the repository's right-pane entity ownership pattern.

### 6. Legacy projection and recovery semantics

First access may lazily project eligible legacy Run Journal data into the read model. A source fingerprint and import watermark make projection idempotent. Parsing accepts the valid prefix and stops at a corrupt tail.

When timestamps or run boundaries are unavailable, records use equal-width sequence geometry and one labeled legacy run if segmentation cannot be proven. Duration, Timing, completion time, results, and run boundaries are never inferred. An active run found after restart becomes interrupted without a fabricated terminal timestamp.

Legacy import reads through an independent connection/source reader and submits durable changes through the ordered writer boundary rather than contending with live capture.

### 7. Chat lifecycle retention

Sanitized history remains while its Chat exists and survives archive and engine restart. Cleanup observes the authoritative workspace Chat set rather than only `DeleteChat`, covering local deletion, synchronized deletion, and Space cascade deletion with one rule.

Deleting a Chat removes store rows, terminates active watches, closes an open surface, and makes stale raw references unavailable. No pruning while a Chat exists is added; database size is diagnosable and a future pruning policy requires a separate product decision.

### 8. Pure projection model and GPUI surface ownership

`TrajectoryViewModel` is the pure authority for run/turn/step groups, lanes, fold domains, search/range focus, selection, timing mode, stable row IDs, viewport anchor, live-edge state, partial/final reconciliation, and ephemeral raw reveal state.

The UI module is split by ownership:

- `view.rs`: GPUI entity, engine watch task, snapshot application, responsive composition.
- `model.rs`: pure projection and interaction state.
- `timeline.rs`: Input/Model/Tools geometry and selection.
- `ledger.rs`: virtualized hierarchical rows and anchoring.
- `inspector.rs`: Summary/Payload/Result/Schema/Timing and reveal lifecycle.
- `toolbar.rs`: Duration/Turns/Calls/Search controls.

Sequence mode gives semantic operations equal widths. Recorded Duration mode uses only valid ordered timestamps; TTFT/decoding split appears only when all required observations are valid. Errors override visual status without erasing lane/classification.

Turns and Calls are independent folds. Search and range focus dim nonmatches without deleting chronology. Selecting either timeline or ledger updates the other and scrolls a stable semantic row into view. Historical prepend preserves anchor and pixel offset. Live append follows only at the live edge and otherwise increments an unread/live affordance.

At sufficient width the ledger and inspector split inside Trajectory. At narrow widths the selected record opens in an internal detail state with a return path. The global Details sidebar is untouched.

### 9. Existing shell owns entry and navigation

Extend the current titlebar capability helper, `RightSurface`, per-panel tab map, and existing open/focus/close paths. The control is content-sized beside Capture and appears only for an available selected main Chat. Registration keys by Chat identity, so repeated activation focuses one surface while separate Chats retain separate entities.

The integration reuses current right-pane width tween, expand, reorder, close fallback, and narrow takeover behavior. Closing destroys the surface/watch but leaves capture active. Chat switches retain per-Chat presentation state; Chat deletion closes it.

### 10. Deterministic surface verification

Core capture-gated fixtures land with the UI model so the surface can be inspected before shell integration. Additional final fixtures cover multi-run, live partial/final replacement, legacy sequence-only, selected tool error, sanitized and unavailable raw states, degraded storage, narrow layout, and multiple Chats. Fixture controls are inert unless `ZERON_UI_CAPTURE=1`.

Focused contract tests cover pure projection, persistence/reopen, fail-open degradation, concurrent read/write access, deletion paths, legacy idempotence, RPC framing/local-only routing, nested raw resolution, viewport behavior, and shell deduplication. Final acceptance requires observing the native GPUI app at standard/narrow widths and dark/light themes; Rust tests alone do not prove placement, contrast, focus, or interaction.

## Data and Control Flow

1. A harness adapter emits normalized `AgentEvent` data.
2. Run Journal assigns/persists source sequence for recovery.
3. `SessionsEngine::publish` continues existing event publication and enqueues a bounded semantic Trajectory projection.
4. The ordered writer persists sanitized records and emits store deltas/watermarks.
5. A local RPC watch returns ordered historical frames and live deltas to one open `TrajectoryView`.
6. The view's pure model derives timeline, virtual ledger, and inspector state.
7. Explicit Reveal sends the selected opaque reference to a local-only handler, which owner-checks and resolves one field from Run Journal in a blocking worker.
8. The view holds the returned raw value only until its lifecycle clear condition.

## Failure and Compatibility Behavior

- Unsupported or failed migration: Chat execution continues; Trajectory opens with a typed degraded/unavailable state.
- Queue saturation or transaction failure: the affected watermark range is degraded; no false completeness.
- Snapshot/delta gap: client requests a resnapshot; no silent reordering.
- Missing/corrupt raw source: Reveal returns unavailable; no transcript fallback.
- Legacy corrupt tail: retain valid prefix and mark incomplete remainder.
- Restart during active run: mark interrupted without invented completion.
- Chat deletion during watch/reveal: terminate and clear state deterministically.
- Existing profiles without a Trajectory database migrate lazily; existing Chat, sync, export, and recovery formats remain compatible.

## Risks / Trade-offs

- **Write overhead:** semantic projection and SQLite commits add local work. Bounded enqueue, one writer, batching, and chunk coalescing protect publication latency at the cost of an explicitly degraded gap under overload.
- **Local growth:** retention-until-delete is predictable and matches the confirmed product decision but may grow for long-lived Chats. Diagnostics are included; pruning is deferred rather than guessed.
- **Raw lookup cost:** Run Journal scans can be expensive. Blocking execution and line guards protect async RPC availability; no persistent raw index is added because it would duplicate sensitive data.
- **Deletion fan-out:** relying on one RPC would leak rows. Watching the authoritative Chat set centralizes cleanup but requires careful startup reconciliation and cascade coverage.
- **GPUI complexity:** timeline, virtualization, inspector, and responsive behavior create coupled interaction state. Pure projection/geometry tests and deterministic fixtures keep rendering thin.
- **Concurrent shell changes:** `shell.rs` and titlebar code are active integration points. Implementation phases serialize shared-file ownership and avoid unrelated refactors.
- **Publication gate:** `.no-mistakes.yaml` is missing although repository policy requires an enforceable push gate. Local change authoring and implementation may proceed, but push/merge/release remain blocked until restored or explicitly replaced.
