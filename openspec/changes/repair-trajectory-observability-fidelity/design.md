## Context

See `proposal.md` for motivation. The correction plan is `docs/plans/2026-09-06-0304-fix-trajectory-observability-fidelity-plan.md`; R/U/KTD references below resolve there.

Current capture emits separate start/result records; UI selection reads one record. `source_seq/sub_seq` is event position, while `rev` is the update cursor. The existing bounded writer, independent readers, legacy cutover and local Reveal authorization are load-bearing constraints. The OMP normalizer discards fields before journal capture. The local OMP source exposes turn/message/tool events and `get_state.dumpTools`, but source and installed executable versions have diverged.

Privacy boundary: ADR 0004 owns the sanitized read model; ADR 0005 requires explicit opt-in for a separate complete diagnostic source. Existing journal/transcript behavior remains unchanged, including whatever normalized data those products already contain.

## Goals / Non-Goals

**Goals:** One shared operation projection for all readers; observable boundaries and provenance; loss-aware diagnostics; replay-safe enrichment; protected bounded source retention; stable selection across live updates and pages.

**Non-Goals:** Replacing recovery, broad event-journal refactoring, synchronized/raw export, global environment capture, provider HTTP interception, Worker trajectories, new billing telemetry or new runtime dependencies. This change does not implement the concurrent OMP chat-runtime-parity plan.

## Decisions

### D1. Shared operation derivation with indexed history lookup

Immutably retain event IDs/order and derive operation identity from profile, Chat, run, complete parent scope and call ID. `crates/proto/src/trajectory.rs` owns correlation/precedence, consumed by engine and UI. Repeated authoritative updates enrich a call without restarting it. Same-scope identity ambiguity is visible rather than silently selecting one result. Run Done does not fabricate tool success.

Add local-only `GetTrajectoryOperation` for a selected persisted Chat/Record ID. Resolve scoped identity server-side, use an index and a consistent snapshot revision, and return sanitized operation data, event-source references, completeness and bounded continuation. Missing loaded pages are loading/incomplete, not evidence of no result. New watch revisions invalidate older operation snapshots; stale query responses cannot replace newer outcomes or another selection. Do not scan/load an entire Chat to inspect one operation.

Alternative rejected: overwrite start rows or join only visible UI rows. The first conflates event and operation identity; the second fails across pages. Avoid copying payloads or quadratic correlation on every render. Covers U1/KTD1–3.

### D2. Separate internal metadata before fan-out

Extend the internal harness-to-engine stream with an envelope carrying the normalized event plus optional diagnostic metadata. Metadata is extracted before the existing AgentEvent journal/transcript/broadcast paths. The normalized event receives its existing journal sequence; the local diagnostic reference binds to that identity. Metadata-only boundaries have deterministic identity and ordering and do not inject duplicate user content into Transcript.

Full original arguments/results never become serializable public AgentEvent fields. The engine owns consent, and the adapter receives an explicit per-run capture policy. When disabled or revoked, do not clone/queue full values. Revocation is also enforced at the writer boundary so already-queued post-revocation work cannot bypass the policy. Other harnesses can provide absent metadata without changing their execution behavior; all internal stream consumers migrate in one cutover.

Alternative rejected: append arbitrary raw fields to AgentEvent or expand recovery JSONL into a diagnostics API. Both widen leakage and couple recovery to a new product. Covers U2/U5/U6/KTD4.

### D3. Boundary vocabulary follows consumed input

A Trajectory Turn starts at consumed user input; OMP turn_start/turn_end represents a model response with its tools and maps to Step. IDs are derived from observed boundaries, not render-time UUIDs or prose. ACK/enqueued steering is not consumed input. Track parent scopes independently. Unknown historical/runtime boundaries remain unknown, not fabricated numbered turns/steps.

Calls folding hides only tool events and preserves interleaved text/reasoning; Turns folding and manual overrides remain independent. Alternative rejected: treat all OMP turns as user turns or reuse one synthetic step. Covers U2/KTD5. Integrate with the owner of `docs/plans/2026-09-06-0238-feat-omp-chat-runtime-parity-plan.md` where normalizer/steering paths overlap.

### D4. Timing supports instants and intervals

Store observed wall-clock position and, where measured, monotonic elapsed time; distinguish runtime execution duration from host capture interval. End-only is an instant. Correlated start/end can form an observed interval without claiming exact execution timing. Render Recorded per measured run/segment; sequence-only portions stay separately labeled. A clock regression invalidates only the affected measurement, not the entire timeline.

Alternative rejected: require startedAt everywhere, synthesize zero durations, or use equal-width fallback for all runs after one missing timestamp. Covers U3/KTD6.

### D5. Usage separates occupancy, consumption and availability

Use authoritative final assistant usage once per stable scoped message; duplicate message_end/turn_end and replay do not add usage. Context occupancy is a last-known observed snapshot from contextUsage, not input consumption. Retain provider/runtime source and optional input/output/cache/total independently; follow source definitions to avoid adding cached tokens twice. Absent or partially known consumption is not a measured zero. Tool rows do not inherit run token totals.

The normalized existing Chat context indicator remains last-known and is not reset by missing updates. Historical synthetic OMP zeros become unknown only with verified provenance; legitimate zero values from other sources are preserved. Alternative rejected: keep finish_agent_end's literal zero as a measurement or sum cumulative session totals as run deltas. Covers U4/U7/KTD7.

### D6. Engine-owned consent and isolated diagnostic source

Consent is ephemeral, specific to the next run of one local Chat/profile, consumed at run start and not rearmed by restart. Closing the view does not affect capture; explicit revocation ends new complete writes without deleting existing source data. A separate local delete-diagnostics action removes complete source and invalidates revealed data while preserving semantic history/recovery.

`crates/engine/src/trajectory_diagnostics.rs` owns a profile-local source separate from sanitized SQLite and Run Journal. Use owner-only directory/file permissions, opaque references, bounded nonblocking queue, one ordered writer and bounded independent reads. No client-supplied source path; symlink/path escape must not bypass the profile root. The source is not a sync/backup product. No bespoke cryptography or guarantee against root/same-user processes is introduced.

Pin limits from the plan: 7-day expiry from capture, 128 MiB total retained source per profile, 1 MiB per captured field. Store original size/fidelity and report truncation or unavailable when a limit prevents full retention. Expiry/oldest-first eviction applies only to complete source, not semantic records or existing journals. A single bounded retention worker handles expiry/budget/deletion. Failed writes close complete capture with a visible gap, never the agent run.

Alternative rejected: global persistent toggle, unlimited JSONL, raw fields in the sanitized store, or turning capture on when the inspector opens. Covers U5/KTD9–11.

### D7. Versioned source references and explicit Reveal

Reuse existing local-only Reveal authorization: executing device/Chat ownership, reference attached to persisted record, exact source version, run/call/parent/field match. Add a diagnostic-source variant and typed fidelity/availability outcomes; old journal-backed references stay readable as normalized legacy data. Bound reads outside the async hot path and recheck lifecycle before returning/applying content. Changes to selection/view/profile/Chat or source deletion invalidate pending/revealed state. Watch carries only sanitized previews and opaque references.

Capture full args at tool_execution_start and result object at tool_execution_end before normalization, under D6 consent. Preserve the original received representation; a runtime-summarized result or external artifact pointer is not unlimited stdout. Do not automatically dereference arbitrary paths. Never log payload-bearing errors.

Schema snapshots use `get_state.dumpTools` after host-tool registration and at catalog-changing boundaries, extracting only necessary tool fields, never systemPrompt. Deduplicate by content; attach provenance and observation time. Preserve the snapshot associated with a run/step; do not claim execution-exact version when the protocol offers only an earlier observation. Sanitized schemas omit sensitive descriptions/examples/values; complete schema source obeys consent and Reveal. Missing runtime schema is unavailable, not static normalized type text.

Alternative rejected: reconstruct schema from arguments, fetch the current catalog to label old calls, or render the normalized ToolCall as original input. Covers U6/KTD8.

### D8. Additive enrichment with revision-safe convergence

Operation correlation applies over existing event rows without rewriting them. Add a separate versioned enrichment marker rather than resetting legacy one-shot imports. Enrich only facts still present in eligible local source; patches preserve identity and use the ordered writer's monotonic rev. Resume in bounded batches after a crash; malformed/missing sources leave existing history readable with a localized gap. Do not overwrite newer native fields or change cutover coverage.

Alternative rejected: delete/rebuild Trajectory at boot or rewrite journals. Missing old schemas/arguments remain not captured; old timing absent from source stays sequence-only. Covers U7/KTD12.

## Risks / Trade-offs

- Installed OMP capability differs from reference source → U2/U4/U6 must inspect frames of the actual executable in an isolated run. Lack of schema/usage/boundary source blocks the affected fidelity acceptance; no invented endpoint or silent fallback. Any upstream runtime adaptation needs explicit authorization, not edits to the read-only reference checkout.
- Complete diagnostics can contain secrets → consent, sanitized defaults, ownership, no forwarding, finite retention and no raw fan-out; independent security review before accepting U5/U6. Owner-only files do not protect against root/same-user processes or copied backups; deletion is not secure erase.
- Late frames/reconnect/page boundaries → scoped IDs, snapshot rev and explicit partiality; test result-before-start, duplicate updates, late completion and cross-page inspection.
- Concurrent work in proto/harness/sessions/inspector → one integration owner and explicit file ownership per batch; do not assume clean HEAD contains the audited dirty checkout. No broad restore, staging or absorption of unrelated changes.
- Storage/performance → no synchronous new I/O in publish, no per-token get_state, no per-event schema copies, no lock held while waiting for a slow watch client.
- Static demo mistaken for proof → U8 records binary/profile identity and observes the adapter-to-UI path in native GPUI, not just a fabricated inspector fixture.

## Migration Plan

1. Before code edits, resolve actual checkout/baseline and native instructions, inspect active overlapping work, and validate installed runtime capabilities without restarting the hosting app. Create sanitized fixtures matching the source shape.
2. Land shared projection/local operation lookup, then diagnostic envelope/boundaries. All optional wire fields decode old records. New local RPC unavailable on an older daemon is reported as unsupported, not authoritative empty data.
3. Integrate timing/usage. Integrate storage/control/Reveal with capture disabled until the privacy path passes; only then attach complete source producers and schema extraction.
4. Apply additive storage migrations and lazy bounded enrichment. Preserve native write coverage, journal bytes, legacy imports and rev semantics. Exercise reopen/crash/replay in a temporary profile.
5. Observe native GPUI and complete itemized conformance R1–R14. Update only affected current contracts/DOX/inventories; archive is a separate authorized completion step.

Rollback: disarm complete capture and prevent new writes before reverting code. Preserve semantic/journal data; do not downgrade schema destructively or reset a store. If the prior binary cannot read a new additive schema/source version, use a previously saved isolated verification profile to validate rollback and keep production data intact pending a compatible reader. Old code must never interpret unknown raw references as journal paths. Publication and any hosting-process restart remain separately authorized operations.
