# Execution Contract

Current plan: `docs/superpowers/plans/2026-09-22-trajectory-repair.md` (T01–T16, R1–R22). `specs/chat-trajectory-preview/spec.md` owns behavior; `design.md` owns decisions. The 2026-09-06 plan is historical context, not a second queue. Existing R1–R14 are preserved; the 2026-09-22 audit adds R15–R22.

This is a planning revision. All implementation checkboxes remain unchecked. Test names, commands and performance targets below are future verification requirements, not executed results. The current user request does not start implementation, commit, publication or restart of the hosting process.

## Execution and Review Rules

- Follow Context → Specification → TDD → Delegation → Implementation → Proof → Review → Validation → Acceptance → Publication → Documentation. Each task includes its own red/green or native before/after check; broader integrated checks belong to section 16.
- The plan's dependency table controls ordering. T03/T04 and T09/T10 may proceed independently only with disjoint file ownership; shared proto/harness/sessions/inspector changes have one integration owner per batch. Map callers before changing exported symbols. Do not absorb unrelated work or share Cargo targets between checkouts.
- Use the existing project's worker/ticket workflow when implementation is authorized. Review each delivery independently against Spec, correctness and Reality; security review is mandatory for preview repair and complete-source boundaries. A green build or worker terminal status is insufficient evidence.
- Filtered tests must execute at least one matching test. Native-only GPUI scenarios require actual typing/clicking/scrolling in an isolated identified executable/profile, not just calls to pure model methods. Record exact commands, counts, exit statuses and native observations without user secrets.
- Publication gates remain not yet due until publication is authorized. Never push upstream. Never restart the app hosting active work as a side effect of validation.

## 1. T01 — Baseline and source characterization

- [ ] 1.1 Resolve actual checkout/HEAD, unrelated edits, active ownership, applicable DOX and installed OMP executable/version. Map affected stream/RPC/projection callers through graft/LSP and confirm current source if stale. Verify by recording concrete baseline, caller map and native profile/launch method without changing the hosting process.
- [ ] 1.2 Establish actual consumed-input/model/tool/usage/schema frames in an isolated OMP run; extract only necessary metadata, never dump systemPrompt or credentials. Files: `crates/harness/tests/omp_rpc.rs`, `tests/fixtures/`. Verify supported capabilities and record exact missing capability for any blocked delivery; no fabricated source fallback.
- [ ] 1.3 Add minimal sanitized source-shaped fixtures for separated call/result, repeated updates, reverse concurrent results, missing hierarchy, end-only/mixed timing and context-only usage. Files: harness fixtures and co-located tests; new `crates/engine/tests/trajectory_fidelity.rs` when crossing layers. Verify each fixture reaches a real consumer boundary and reproduces an audited fault; native search/scroll baseline is also recorded.

## 2. T02 — Legacy sanitization and existing-preview repair

- [ ] 2.1 Add the failing `trajectory_legacy_preview_redacts_before_reveal` test from plan T02 for text and reasoning, plus an old persisted-row fixture. Files: `crates/engine/src/trajectory_store.rs`, `crates/engine/tests/trajectory_fidelity.rs`. Verify sentinel leakage reproduces before the fix, while recovery source bytes remain unchanged.
- [ ] 2.2 Use the existing sanitizer's preview in both legacy coalescers and protect ordinary reader/watch output for already-persisted rows before backfill. Files: store/rpc. Verify new import and first snapshot/watch from an old store contain no sentinel without requiring repair completion or Reveal.
- [ ] 2.3 Add versioned bounded ordered-writer repair of existing preview text with monotonic rev, independent of cutover/import markers. Verify interruption/retry/idempotence, concurrent native capture, unchanged IDs/references/journal bytes, direct persisted rows after completion and update delivery to an earlier subscriber; privacy review accepts the complete read/write path.

## 3. T03 — Editable search and technical identifiers

- [ ] 3.1 Add failing `trajectory_search_finds_call_identity` and identifier/text cases in `crates/ui/src/trajectory/model.rs`; reproduce the non-editable field natively. Verify call/parent/run/turn/step/record IDs, case-insensitive sanitized text and clear semantics; raw revealed content is excluded from search.
- [ ] 3.2 Replace the decorative search field with the existing picker-style native input owned/subscribed by `TrajectoryView`, connected to Search/ClearSearch. Files: `view.rs`, `toolbar.rs`. Verify actual typing, paste, text selection and clearing update both input/model, preserve live query/focus and never submit a Chat message.
- [ ] 3.3 Validate input lifecycle on Chat/surface switches and close; verify no stale subscription or clear-event feedback loop. Run focused model/toolbar tests and native normal/narrow interaction; evidence must include editing the displayed control.

## 4. T04 — Scrollable detail and readable action context

- [ ] 4.1 Reproduce clipped long content with the existing native fixture surface: 200 numbered lines, long identifiers and 599/600 px widths. Files: `crates/ui/src/capture.rs` only as needed, trajectory inspector/view. Verify inability to reach the last line before the fix and retain a repeatable native exercise.
- [ ] 4.2 Give the tab body bounded vertical scroll while header/tabs remain accessible; reset detail position on selection change and preserve it on same-record live update. Files: inspector/view. Verify last-line access in every applicable tab, revealed data, low-height panel and narrow return; selection change still clears Reveal.
- [ ] 4.3 Prioritize sanitized action/target/outcome in ledger/Summary while preserving complete IDs as accessible metadata and 26 px rows. Files: model/ledger/inspector. Verify pure label/summary tests and native light/dark presentation, missing-data honesty and distinction between available run terminal outcome and subordinate errors.

## 5. T05 — Shared scoped operation projection

- [ ] 5.1 Add failing `trajectory_operation_contract` cases in `crates/proto/src/trajectory.rs`: success/error, reverse concurrency, result-only, late result, Done without result, repeated metadata, cross-run/Chat/parent IDs and ambiguous same-scope collision. Verify expectations fail on isolated-event interpretation before implementation.
- [ ] 5.2 Implement one pure operation derivation with authoritative outcome, separate payload/result source identities, completeness and original start preservation. Keep event IDs/order intact; avoid quadratic joining or payload copying per render. Verify all operation cases, replay idempotence and deterministic ambiguity reporting.
- [ ] 5.3 Freeze additive operation/scope/source contracts for downstream consumers; map callers and test serde old/new/unknown fields. Verify `cargo test -p zeron-proto trajectory_operation_contract` executes cases and unchanged normalized event/transcript contracts remain compatible.

## 6. T06 — Indexed query and correlated Inspector

- [ ] 6.1 Add failing `trajectory_operation_lookup` with call/result in different pages separated by 20,000 events, late completion during lookup and selection/profile invalidation. Files: engine store/rpc and integration test. Verify no whole-Chat scan/load is required and missing pages are not interpreted as absent results.
- [ ] 6.2 Implement indexed local-only `GetTrajectoryOperation` for a persisted Chat/Record ID, consistent snapshot revision and bounded continuation. Files: RPC lib/method, engine store/rpc. Verify ownership, forged IDs, omitted targetDeviceId, remote forwarding rejection and explicit unsupported behavior with an older daemon.
- [ ] 6.3 Connect Inspector and live invalidation to the shared operation while preserving the selected event. Files: UI model/view/inspector. Verify sources differ correctly for Payload/Result Reveal, newer rev defeats stale responses, selection-before-completion updates without reclick and reopened call/result inspections agree.
- [ ] 6.4 Run RPC round-trip/local-only, `trajectory_operation_lookup` and UI `trajectory_operation_contract`; verify native cross-page operation and delayed-response selection cases. No raw source is fetched merely by selecting an operation.

## 7. T07 — Observed hierarchy and internal envelope

- [ ] 7.1 Add failing `trajectory_hierarchy_contract` from consumed input, two model responses, steering ACK then consumption and interleaved parent scopes. Files: OMP RPC fixtures, sessions/proto/UI tests. Verify expected Turns/Steps, no transcript duplication and replay-stable boundaries.
- [ ] 7.2 Add internal harness→engine metadata envelope before public AgentEvent fan-out and migrate every resolved stream consumer. Files: harness lib/OMP, sessions/proto and mapped callers. Verify other harnesses accept absent metadata, raw bodies do not enter public/recovery events and metadata-only ordering is deterministic.
- [ ] 7.3 Capture stable observed turn/step/full-parent identities, coalescers per scope and honest unknown groups. Files: sessions/store/proto/UI model. Verify ACK does not open a Turn, model response maps to Step, nested scope isolation and native unknown/observed grouping; run OMP, trajectory_capture and hierarchy tests.

## 8. T08 — Calls folding, selection and timeline dimming

- [ ] 8.1 Add failing `trajectory_navigation_contract` for interleaved reasoning/tool/text/tool, independent Turn/Call overrides, selected hidden child and query+fold. Files: model/timeline/ledger. Verify old step-wide fold and visible-row-only dimming fail the new behavior cases.
- [ ] 8.2 Separate tool folding from Step content and open only the required ancestor path on explicit selection; preserve unrelated overrides, event identity and global order with concurrent results. Files: model/view/ledger. Verify selected hidden record becomes a visible selected row and model content survives Calls folding.
- [ ] 8.3 Derive dimming from query/range over all loaded records, independent of folded rows. Files: model/timeline. Verify the same match set before/after folds, then native search→fold→timeline click→clear with offscreen records at both widths.

## 9. T09 — Honest recorded timing

- [ ] 9.1 Add failing `trajectory_recorded_contract` for start-only/end-only observations, final message, overlapping operations and a separate sequence-only legacy run. Files: timeline/proto tests. Verify invalid or regressing local timing does not discard valid timing elsewhere.
- [ ] 9.2 Preserve first capture start through finalization, record timing provenance and available monotonic elapsed; derive correlated host intervals separately from runtime execution duration. Files: sessions/store/proto. Verify missing versus measured-zero duration, overflow and differing runtime/host values.
- [ ] 9.3 Render measured instants/intervals plus explicitly sequence-only segments; label effective/mixed mode and preserve range/selection/hit-testing. Files: timeline/inspector/toolbar. Verify timing regression tests and native Sequence/Recorded on the same mixed history with overlap/end-of-axis selection.

## 10. T10 — Consumption and context fidelity

- [ ] 10.1 Add failing `trajectory_usage_fidelity` through adapter/capture/store/reopen: context 109686/272000 with unknown consumption, literal old OMP zeros, measured zero and duplicate final-message usage. Verify absent and zero differ and known context survives.
- [ ] 10.2 Extract final-message usage once per scoped identity separately from context snapshots, with source/availability/cache semantics. Files: OMP mod/normalize/protocol, proto, sessions/store. Verify message_end/turn_end replay, resume, cumulative totals, subordinate duplication and compaction/model-change snapshots.
- [ ] 10.3 Display consumption/context at the model/Step/run scope, retain partiality and keep Chat's last-known indicator. Files: inspector and affected mapped consumers. Verify no tokens inherited by unrelated tools, run focused usage/OMP tests and compare native output with actual supported runtime frames.

## 11. T11 — Consent and local diagnostic contracts

- [ ] 11.1 Add `trajectory_diagnostic_consent` behavior cases before implementation: Off→Armed→Capturing→Off, Chat/profile scope, next-run consumption, another Chat, restart, view close, disarm and active revocation. Files: engine sessions/rpc tests. Verify each state/transition and that revocation cannot be bypassed by queued writes.
- [ ] 11.2 Implement additive source/fidelity/availability contracts and engine-owned local control policy. Files: proto, RPC lib/method, sessions/rpc. Verify old journal references still decode, forged/foreign/forwarded controls fail and an older daemon reports unsupported; complete source producers stay disabled until T12 passes.
- [ ] 11.3 Add visible scope/limits/off/armed/capturing/revoke/delete controls in Trajectory. Files: toolbar/model/view. Verify native next-run scope with two Chats and restart/profile clearing; opening the view neither arms capture nor reveals content.

## 12. T12 — Private source, retention and authorized Reveal

- [ ] 12.1 Add and implement `trajectory_diagnostics` source tests for owner-only filesystem access, safe opaque reference paths, bounded queue/writer/readers and typed failure. Files: new engine trajectory_diagnostics.rs plus lib/profile/sessions. Verify queue/disk/writer failure exposes diagnostic gaps while normalized execution/journal continue.
- [ ] 12.2 Implement and test seven-day expiry, 128 MiB/profile oldest-first eviction, 1 MiB/field cap and original-size/fidelity metadata with injected clock/limits. Files: diagnostics/workspace_host/rpc. Verify serial retention, archive semantics, explicit diagnostic deletion and local/sync/Space Chat deletion preserve appropriate semantic/recovery data.
- [ ] 12.3 Extend exact persisted-reference/version/field/ownership checks to diagnostic Reveal outside the async hot path; recheck lifecycle before returning. Files: engine rpc, RPC lib/method/tests/device_room, UI view/inspector. Verify cross-profile/forged/remote denial, transient retry and pending/revealed invalidation on source expiry/deletion, selection, close, Chat or profile change.
- [ ] 12.4 Run diagnostic and `trajectory_reveal`/relay tests plus native off/armed/capturing/revoke/delete/Reveal flows. Independent privacy review must accept no-leak/authorization/retention/fail-open evidence before enabling complete source producers.

## 13. T13 — Runtime schema and original received data

- [ ] 13.1 Add `trajectory_source_fidelity` fixtures: catalog A→B, args with command/cwd/env/timeout, structured multiform result, sensitive examples and missing capability. Files: OMP RPC fixtures and engine tests. Verify expected historical source, sanitized defaults and explicit absence before producer implementation.
- [ ] 13.2 Extract only necessary get_state.dumpTools schema after host-tool registration and catalog changes, deduplicated with observation provenance; omit systemPrompt. Files: OMP mod/protocol/process, proto, sessions/store. Verify actual installed-runtime snapshots, precision and unsupported capability without static-schema substitution.
- [ ] 13.3 Capture original received args/result before normalization only under active policy; enforce no unnecessary raw clone/queue when off/revoked and bounded preservation of text/details/diff/image metadata/reference. Files: OMP normalize/mod, sessions/diagnostics. Verify opt-in field fidelity without following artifact paths or collecting inherited environment/credentials.
- [ ] 13.4 Present source/fidelity/availability in schema/payload/result and re-review the integrated privacy boundary. Files: inspector/view/proto/rpc. Verify real runtime Reveal and a diagnostic-only sentinel absent from public AgentEvent, recovery additions, SQLite/watch, sync/export, Voice and logs; unsupported source leaves the affected fidelity acceptance pending.

## 14. T14 — Non-destructive history enrichment

- [ ] 14.1 Add failing `trajectory_fidelity` integration with old-format store, separate sources, known context, proven synthetic zeros, legacy import marker, newer native data and subscriber at old rev. Verify the expected before/after facts and recovery byte equality.
- [ ] 14.2 Implement versioned lazy bounded enrichment independent of legacy-import and sanitization-repair markers, preserving identity/order and newer fields. Files: store/run_journal/proto. Verify idempotence, interrupted resume, missing/corrupt/oversized sources and no destructive rebuild; unrecoverable schemas/args/measurements stay absent.
- [ ] 14.3 Deliver patches through ordered writer/rev and test concurrent native writes, reconnect and reopen. Files: store/rpc, trajectory_fidelity.rs/restart_resume.rs. Verify `cargo test -p zeron-engine --test trajectory_fidelity`, `--test restart_resume` and `trajectory_watch`; no duplicate rows or journal rewrite.

## 15. T15 — Quiet live navigation and measured scale

- [ ] 15.1 Reproduce quiet-stream scrollback; add `trajectory_live_navigation` cases for explicit return, user movement versus pending jump, selection offscreen and stream faster than frame rate. Files: view/ledger/model/toolbar. Verify actual position-based behavior across wheel/trackpad/drag/keyboard and no implicit rearm.
- [ ] 15.2 Keep explicit live-edge return available without a new watch item; retain the watch-side pre-catch-up guard, pending-jump distinction and two-row tolerance. Verify native scrollback with zero incoming events and selection during a pending live jump; pending counts exclude revisions of existing rows.
- [ ] 15.3 Benchmark synthetic 20,373-event/12-run history with 10 deltas/s for 60 seconds on an identified optimized build/reference machine. Files: existing capture fixture and affected projection/render functions. Verify p95 input/selection feedback below 100 ms and no UI stall above 250 ms; record baseline and final measurements, not inferred performance.
- [ ] 15.4 Optimize only measured projection/render bottlenecks with revision/query/fold invalidation and bounded/density-aware timeline painting if needed. Preserve virtualized ledger, deterministic event hit mapping, search/errors/selection and indexed operation lookup. Verify pure geometry/projection regressions and repeat the same native benchmark when changes are necessary.

## 16. T16 — Integrated proof, review, acceptance and documentation

- [ ] 16.1 Consolidate source-shaped adapter→capture→store→watch→projection/inspector integration, including failure, concurrency, pages, stale response, replay and reopen. Files: trajectory_fidelity.rs, omp_rpc.rs, co-located UI tests. Verify plausible behavior faults fail before fixes; perfect synthetic inspector records alone are insufficient.
- [ ] 16.2 After focused suites pass and concurrent mutations settle, run `cargo fmt --all -- --check`, `cargo build`, `cargo test --workspace` and canonical `scripts/e2e-smoke.sh` after reading its DOX. Verify exact commands/exits/test counts; resolve affected failures and report external blockers without declaring passed checks.
- [ ] 16.3 Execute all T16 native scenarios from the plan with actual supported OMP source plus isolated synthetic UI/scale fixtures, in light/dark, 599/600 px and low-height layouts. Verify editable search, folds/hidden selection, detail last line, quiet follow, timing/usage/schema/consent/Reveal and lifecycle using identified binary/profile; no hosting-app restart.
- [ ] 16.4 Complete independent Spec, correctness/regression, privacy and Reality reviews of the integrated diff. Verify all R1–R22 have concrete test/native evidence in the ledger below; return blockers to their delivery instead of marking tasks complete on worker status alone.
- [ ] 16.5 Update affected DOX/Test Coverage Matrix and current CONTEXT/architecture/design/functional/changelog/streaming-guide contracts only where behavior changed. Validate OpenSpec; archive only after implementation acceptance. Verify docs reflect actual native evidence, unrelated work is untouched and publication remains separately gated to the fork.

## Conformance Ledger

Pending entries are deliberate implementation gaps. Fill evidence only from executed tests and native observations.

| Requirement | Deliveries | Acceptance focus | Evidence |
|---|---|---|---|
| R1 | T05/T06 | Complete operation, selected event retained | pending |
| R2 | T05/T06/T07 | Scope, concurrency, updates, pages and replay | pending |
| R3 | T05/T06 | Missing result, error, result-only, late result | pending |
| R4 | T07/T08 | Consumed Turns, model Steps, independent tool folds | pending |
| R5 | T09 | Instants/intervals, mixed legacy, provenance | pending |
| R6 | T10 | Context/consumption/zero/cache/dedup | pending |
| R7 | T13 | Observed schema and historical precision | pending |
| R8 | T12/T13 | Opted-in original received args/results | pending |
| R9 | T11/T12 | Next-run Chat/profile consent and clearing | pending |
| R10 | T12/T13 | No diagnostic raw in public/normalized channels | pending |
| R11 | T06/T12/T13 | Authorized references, bounds, lifecycle invalidation | pending |
| R12 | T14 | Recoverable history without reset/journal rewrite | pending |
| R13 | T06/T14/T16 | Live/reopen/rev/pages, stable selection/scroll | pending |
| R14 | T12/T14 | Bounded fail-open capture and retention | pending |
| R15 | T03 | Native editable search, IDs and clear | pending |
| R16 | T08 | Hidden selection reveal and fold-independent dimming | pending |
| R17 | T04 | Reachable long Inspector content and narrow return | pending |
| R18 | T02/T14 | Legacy import/read/backfill sanitization | pending |
| R19 | T04/T06 | Useful sanitized action summary with full metadata | pending |
| R20 | T15 | Explicit return on quiet stream, no implicit rearm | pending |
| R21 | T15 | Measured large-history responsiveness and identity | pending |
| R22 | T16 | Native interaction and actual runtime chain evidence | pending |
