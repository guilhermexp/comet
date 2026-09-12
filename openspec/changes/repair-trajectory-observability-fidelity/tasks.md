# Execution Contract

Source plan: `docs/plans/2026-09-06-0304-fix-trajectory-observability-fidelity-plan.md`. R1–R14 and U1–U8 retain their plan meanings. `specs/chat-trajectory-preview/spec.md` is the behavioral acceptance contract; `design.md` owns implementation decisions.

This change is authored and validated only. No unchecked implementation task is evidence of work already performed. Commands and regression names below are execution targets, not recorded results. User authorization to author this change does not authorize starting workers, committing, publishing or restarting the hosting Comet process.

## Phase Map

| Phase | Plan unit | Section | Dependencies | Boundary | Acceptance evidence |
|---|---|---|---|---|---|
| P0 | Preflight | 1 | — | Real checkout/runtime and sanitized characterization | Source capability evidence; baseline and ownership map |
| P1 | U1 | 2 | P0 | Shared operation projection, indexed query, inspector | Correlation regression and selection/reveal-source evidence |
| P2 | U2 | 3 | P1 | Internal envelope, consumed boundaries, folds | Runtime-to-group fixture and folding evidence |
| P3 | U3 | 4 | P1 | Instants/intervals, mixed timing | Timing regression and native geometry |
| P4 | U4 | 5 | P2 | Usage/context fidelity | Final-message replay and context evidence |
| P5 | U5 | 6 | P2 | Consent, private source, local access/retention | Privacy isolation and fail-open evidence |
| P6 | U6 | 7 | P1, P2, P5 | Original source and schema snapshots | Actual runtime fidelity and native Reveal |
| P7 | U7 | 8 | P3, P4, P6 | Historical enrichment and convergence | Temporary-profile crash/reopen/replay evidence |
| P8 | U8 | 9 | P1–P7 | Integrated acceptance and scoped contract closure | R1–R14 itemized conformance, native evidence and final review |

Parallel eligibility does not grant shared-file ownership. `proto/trajectory.rs`, `engine/sessions.rs`, `harness/omp/normalize.rs` and `ui/trajectory/inspector.rs` have one integration owner per batch. Independent worktrees must be based on the actual accepted integration state, not an unrelated clean HEAD. The concurrent OMP runtime-parity plan is adjacent ownership, not extra scope.

## Review Contract

- **Spec owner:** orchestrator, with read-only phase review where supported; milestone is every phase before ACCEPT. Evidence maps each phase requirement/must_have to behavior and reviewed worker tests. Incomplete scenarios return REVISE on the same durable ticket.
- **Standards & Security owner:** orchestrator and security reviewer for P5/P6; design/privacy review before those phases are accepted. Publication gates remain `not yet due` during local-only work, never falsely `passed`. Before any authorized push/merge, resolve and run the repository's live security and automated-review gates on the final diff.
- **Reality owner:** orchestrator; milestone is each UI-bearing delivery before reporting it complete, then P8 integrated native smoke. Evidence includes executable/profile identity, exercised scenario and observed output/screenshots. A static capture fixture alone does not prove actual runtime capture.
- Every implementation worker uses a durable Work Ticket and verified completion signature. Terminal completion and green tests are insufficient for ACCEPT. No worker touches the hosting app lifecycle without explicit authorization.

## 1. Preflight and characterization

**must_haves:** the worker edits the intended checkout with unrelated work preserved; source capabilities are known for the installed OMP version; reproduction fixtures model the observed faults without importing user secrets.

- [ ] 1.1 Resolve actual checkout/worktree state, active owners, native instructions and task runner. Record the baseline and shared-file ownership with the concurrent OMP parity work; enumerate internal stream/RPC/record callers through LSP references before changing exported contracts. files: affected domains listed in the phase map, no edits required for discovery. verify: concrete baseline and caller/owner map, not inferred worker status.
- [ ] 1.2 Establish installed OMP version and actual turn/message/tool/usage/schema frames using an isolated local run under the repo's canonical launch method. Extract only necessary metadata from get_state; do not dump systemPrompt or credentials. If required capabilities are absent, stop the affected phase with the exact missing source and request authorization for any runtime adaptation. files: `crates/harness/tests/omp_rpc.rs`, `crates/harness/tests/fixtures/`. verify: supported-version frame evidence and sanitized fixtures; no edits to the read-only reference checkout.
- [ ] 1.3 Build minimal deterministic fixtures matching the audited separated call/result, repeated todo IDs, absent hierarchy, end-only timing and context-only usage shapes. Reuse the existing audit as the symptom baseline; do not rerun production queries merely to reconfirm it. files: `crates/harness/tests/fixtures/`, co-located trajectory tests and new `crates/engine/tests/trajectory_fidelity.rs` where integration is necessary. verify: each fixture exercises a consumer-visible contract; no invented all-fields-filled inspector record substitutes for capture.

## 2. Correlated operation and paged inspector

**must_haves:** call and result resolve one scoped operation across pages; selected event identity remains stable; no result is fabricated and no unrelated operation is joined. Covers R1–R3/R13.

- [ ] 2.1 Add the failing `trajectory_operation_contract` behavior reproduction using separated start/result events and selection on the start. Include success/error, reversed concurrent completion, result-only, late completion and Done without a result. files: `crates/ui/src/trajectory/model.rs`, `crates/proto/src/trajectory.rs`. verify: `cargo test -p zeron-ui trajectory_operation_contract` fails on the characterized contract before the fix and passes after it.
- [ ] 2.2 Implement shared scope-aware operation derivation and explicit identity ambiguity. Repeated metadata updates/replay retain authoritative outcome and original start; preserve event ordering/IDs and avoid per-render payload copying or quadratic joining. files: `crates/proto/src/trajectory.rs`, `crates/ui/src/trajectory/model.rs`. verify: co-located behavior regressions for same call ID across run/Chat/parent scope and authoritative todo updates.
- [ ] 2.3 Add indexed, authorized, bounded local `GetTrajectoryOperation` for a persisted selected Record ID, returning sanitized operation data, source references, snapshot revision and completeness/continuation. Preserve ownership/forwarding limits and distinguish unloaded data from absent result. files: `crates/rpc/src/{lib.rs,method.rs}`, `crates/engine/src/{trajectory_store.rs,rpc.rs}`, co-located tests. verify: cross-page operation resolves without full-Chat loading; foreign IDs rejected; concurrent newer revision wins over stale response.
- [ ] 2.4 Connect inspector tabs and summary to the operation while preserving the selected event. Bind Payload/Result Reveal to their own source references, update live without reselection and keep pending response invalidation intact. Include call-ID search and named tool completion labels. files: `crates/ui/src/trajectory/{model.rs,inspector.rs,view.rs,ledger.rs}`. verify: focused UI model tests plus native selection-before-completion and historical reopen; no response crosses selection/profile/delete boundaries.

## 3. Observed hierarchy and independent folds

**must_haves:** model response boundaries become Steps under consumed-input Turns; absent boundaries remain unknown; Calls folding preserves text/reasoning. Covers R2–R4/R13.

- [ ] 3.1 Add failing `trajectory_hierarchy_contract` using a consumed input, two model responses/tools and a later consumed input. Include steering ACK before consumption and replay. files: `crates/harness/tests/omp_rpc.rs`, `crates/ui/src/trajectory/model.rs`. verify: `cargo test -p zeron-ui trajectory_hierarchy_contract` and harness fixture reproduction defend observed group/fold behavior, not field forwarding.
- [ ] 3.2 Introduce the internal harness→engine envelope and extract metadata before AgentEvent fan-out. Migrate every internal stream consumer; source metadata absent on other harnesses remains valid. Do not add raw bytes to recovery/public AgentEvent. files: `crates/harness/src/lib.rs`, `crates/harness/src/omp/{mod.rs,normalize.rs}`, `crates/proto/src/trajectory.rs`, `crates/engine/src/sessions.rs`, references-resolved consumers. verify: existing harness/transcript/recovery contracts remain valid; diagnostic-only boundaries do not duplicate transcript content.
- [ ] 3.3 Capture stable consumed-input/model/parent boundaries in order and persist metadata for new records. Replay does not renumber groups; unknown groups are clearly labeled. files: `crates/engine/src/{sessions.rs,trajectory_store.rs}`, `crates/proto/src/trajectory.rs`, `crates/ui/src/trajectory/model.rs`. verify: normalizer→capture→group fixture covers interleaved scopes and late boundaries.
- [ ] 3.4 Separate Calls from Step folding and preserve interleaved model records, Turn settings, manual overrides, selected descendants and chronological order. files: `crates/ui/src/trajectory/{model.rs,ledger.rs}`. verify: the original fold reproduction passes and native Calls toggle leaves model content visible.

## 4. Recorded instants and intervals

**must_haves:** end-only observations remain usable; missing legacy timing does not invalidate measured runs; execution and observation duration are distinctly labeled. Covers R5/R13.

- [ ] 4.1 Add failing `trajectory_recorded_contract` with start-only, end-only, final message and a separate legacy sequence-only run. Include reversed clock and overlapping operations. files: `crates/ui/src/trajectory/timeline.rs`. verify: `cargo test -p zeron-ui trajectory_recorded_contract` fails on the old global fallback and passes on mixed honest geometry.
- [ ] 4.2 Capture timing provenance and available monotonic elapsed measurements; derive tool intervals from correlated source without claiming transport interval equals execution duration. Handle invalid/overflowing measurements locally. files: `crates/proto/src/trajectory.rs`, `crates/engine/src/{sessions.rs,trajectory_store.rs}`. verify: measured zero versus absent duration, execution-versus-host difference and local invalid timing cases.
- [ ] 4.3 Render measured instants/intervals and explicitly sequence-only segments with stable selection/range/hit-testing. Update Timing and toolbar labels without losing lane/error semantics. files: `crates/ui/src/trajectory/{timeline.rs,inspector.rs,toolbar.rs}`. verify: focused model/layout tests and native mixed-timing smoke at normal/narrow widths.

## 5. Honest consumption and context

**must_haves:** occupancy 109686/272000 survives; missing consumption is not zero; duplicate final frames/resume do not inflate totals. Covers R6/R12/R13.

- [ ] 5.1 Add failing `trajectory_usage_fidelity` through adapter/capture/store/reopen with context-only usage and synthetic old zeros. files: `crates/harness/tests/omp_rpc.rs`, `crates/engine/src/{sessions.rs,trajectory_store.rs}`. verify: `cargo test -p zeron-engine trajectory_usage_fidelity` fails on lost context/fake consumption and passes after correction.
- [ ] 5.2 Extract authoritative final-message usage once per scoped message and context snapshots separately. Preserve optionality/provenance and cache/total source semantics; never add cumulative session values as run deltas or duplicate subordinate usage. files: `crates/harness/src/omp/{mod.rs,normalize.rs,protocol.rs}`, `crates/proto/src/trajectory.rs`, `crates/engine/src/{sessions.rs,trajectory_store.rs}`. verify: replay, duplicate message_end/turn_end, resume, reported zero, missing/cache fields and compaction snapshots.
- [ ] 5.3 Display consumption/context/partiality at their correct model/step/run scope, not on unrelated tools; preserve Chat's last-known indicator. files: `crates/ui/src/trajectory/inspector.rs`. verify: native context-only and measured-consumption scenarios match actual runtime frames.

## 6. Opt-in private source and authorized Reveal

**must_haves:** consent is next-run/Chat/profile-scoped; raw bodies have no public fan-out; source retention is bounded; revocation/deletion works; failure never interrupts the run. Covers R8–R11/R14. Security-sensitive worker ownership is mandatory.

- [ ] 6.1 Define additive consent, diagnostic-source reference, fidelity/availability and local-control contracts. Implement engine-owned arm/disarm/revoke policy consumed once at matching run start and cleared on restart/profile/deletion. files: `crates/proto/src/trajectory.rs`, `crates/rpc/src/{lib.rs,method.rs}`, `crates/engine/src/{sessions.rs,rpc.rs}`. verify: next-run scope, other Chat, subsequent run, active revocation and queued-write policy behavior.
- [ ] 6.2 Implement isolated profile-local `trajectory_diagnostics.rs` with owner-only permissions, safe source paths, opaque references, bounded writer/readers, typed degradation and no AgentEvent/journal/raw-watch contamination. files: new `crates/engine/src/trajectory_diagnostics.rs`, `crates/engine/src/{lib.rs,profile.rs,sessions.rs,trajectory_store.rs}`. verify: sensitive sentinel introduced only in pre-normalization diagnostic fields is absent from public/normalized paths, permissions/escape checks, queue/writer/disk failure keep execution live.
- [ ] 6.3 Implement 7-day expiry, 128 MiB profile budget, 1 MiB field limit, fidelity/original-size metadata, oldest-first eviction and explicit delete-diagnostics. Integrate all Chat deletion paths and archive semantics using serialized bounded retention. files: `crates/engine/src/{trajectory_diagnostics.rs,workspace_host.rs,rpc.rs}`, co-located tests. verify: TTL/budget/truncation, archive, local/sync/Space deletion, concurrent reader and retention failure do not erase semantic/journal data.
- [ ] 6.4 Extend local Reveal to the new source under exact ownership/attached-reference/version/field checks and lifecycle invalidation; preserve explicitly labeled journal-backed normalized legacy reads. Reject forwarding, forged references and cross-profile calls. files: `crates/engine/src/rpc.rs`, `crates/rpc/src/{lib.rs,method.rs}`, `crates/rpc/tests/device_room.rs`. verify: `cargo test -p zeron-engine trajectory_reveal` plus new diagnostic access tests; source deletion while reading and omitted target device cannot leak.
- [ ] 6.5 Add explicit next-run consent control, visible scope/limits/active state, disarm/revoke and delete-diagnostics affordance inside Trajectory. Keep sanitized previews default and Reveal explicit; invalidate pending/revealed content on all existing lifecycle boundaries. files: `crates/ui/src/trajectory/{toolbar.rs,model.rs,view.rs,inspector.rs}`. verify: native off→armed→capturing→off and revoke/delete flows, view close/reopen and transient Reveal error/retry.
- [ ] 6.6 Review the privacy boundary and worker tests before accepting the phase; keep complete producers disabled until source and Reveal satisfy the contract. files: phase P5 diff and test evidence. verify: itemized no-leak/ownership/retention/fail-open findings with no unresolved blocking issue; publish gates are not falsely marked passed.

## 7. Runtime schemas and original source extraction

**must_haves:** schema has observed provenance; opted-in source preserves received fields before normalization; unsupported/truncated source is not advertised as complete. Covers R7–R11.

- [ ] 7.1 Extract bounded get_state.dumpTools snapshots after host-tool registration and on catalog-changing boundaries, deduplicated by content with source/time precision. Never retain systemPrompt or claim a stale snapshot is execution-exact. files: `crates/harness/src/omp/{mod.rs,protocol.rs,process.rs}`, `crates/proto/src/trajectory.rs`, `crates/engine/src/{sessions.rs,trajectory_store.rs}`. verify: two actual/sanitized runtime catalog snapshots, schema absence and sensitive examples preserve fidelity/preview boundaries.
- [ ] 7.2 Capture original received args and result objects from execution boundaries under active consent before normalization. Enforce revocation without unnecessary raw cloning; retain text/details/diff/image/reference structure within declared limits, without reading arbitrary artifact paths. files: `crates/harness/src/omp/{normalize.rs,mod.rs}`, `crates/engine/src/{sessions.rs,trajectory_diagnostics.rs}`, `crates/harness/tests/omp_rpc.rs`. verify: command/cwd/env/timeout and multiform result survive opted-in Reveal; consent off/revoked retains no complete source.
- [ ] 7.3 Bind schema/payload/result inspector presentation to source fidelity, provenance and explicit unavailability; remove normalized-type-as-schema claims and complete-source claims on legacy data. files: `crates/ui/src/trajectory/{inspector.rs,view.rs}`, `crates/proto/src/trajectory.rs`, `crates/engine/src/rpc.rs`. verify: native complete/legacy/sanitized/truncated/expired/unsupported cases and stale Reveal response isolation.
- [ ] 7.4 Re-review the integrated producer-to-source privacy path, including logs, Voice, watch, export and journal behavior. files: P5/P6 integration diff and focused tests. verify: actual runtime fidelity evidence and no-leak sentinel checks; missing runtime capability blocks this phase rather than accepting static schema fallback.

## 8. Non-destructive historical enrichment

**must_haves:** old calls correlate and recoverable context survives; enrichment is idempotent/restartable and revision-safe; journals and native newer fields remain intact. Covers R12–R14.

- [ ] 8.1 Add `trajectory_fidelity` integration reproduction with a temporary old-format profile and separate call/result/context records. Include existing native coverage, legacy marker and a subscriber at an earlier revision. files: new `crates/engine/tests/trajectory_fidelity.rs`. verify: `cargo test -p zeron-engine --test trajectory_fidelity` fails on the pre-enrichment contract and becomes the green acceptance signal.
- [ ] 8.2 Add versioned lazy bounded enrichment markers/patches without resetting imports, deleting stores or changing source events. Preserve scoped correlation and only recover facts still in local source; treat source-proven synthetic zeros specifically, not all zeros. files: `crates/engine/src/{trajectory_store.rs,run_journal.rs}`, `crates/proto/src/trajectory.rs`. verify: idempotent rerun, missing/corrupt/oversized journal and partial recovery with existing history preserved.
- [ ] 8.3 Integrate revision-safe enrichment delivery and reopen/crash behavior. Protect newer native information, full watermark semantics and existing recovery behavior. files: `crates/engine/src/{trajectory_store.rs,rpc.rs}`, `crates/engine/tests/{trajectory_fidelity.rs,restart_resume.rs}`. verify: integration reproduction passes; `cargo test -p zeron-engine trajectory_watch` and restart/resume contract remain green; journal source bytes are unchanged in the temporary fixture.

## 9. Integrated acceptance and contract closure

**must_haves:** actual source→adapter→store→watch→projection→inspector behavior is observed; all R1–R14 have evidence; no runtime parity/retention/security gap is hidden by a green build.

- [ ] 9.1 Consolidate consumer-visible integration coverage and replace misleading all-fields-filled fixtures where they bypass the broken boundary. Keep regression tests that defend plausible behavior faults; do not add source-text/default assertions merely for coverage. files: `crates/engine/tests/trajectory_fidelity.rs`, `crates/harness/tests/omp_rpc.rs`, co-located `crates/ui/src/trajectory/{model.rs,inspector.rs,timeline.rs,view.rs}` tests. verify: separated source-event shape exercises the integrated path; duplicate replay/revisions converge.
- [ ] 9.2 Resolve and run the repo's canonical focused suites, then integrated formatting/build/workspace gates once after concurrent mutations settle. Inspect native instructions and live command help before gates; a filtered command matching zero tests is not evidence. files: final affected code/tests. verify: executed commands and exits recorded, including focused bug signals and complete integration/build results; no hidden pipe failure.
- [ ] 9.3 Launch the canonical isolated native GPUI verification surface with executable/profile provenance. Exercise live selection before completion, history/restart/reconnect, cross-page call/result, parallel tools/error/subagent, folds/scroll/range, mixed timing, usage, schema and complete-source off/on/revoke/delete. files: existing fixture surface `crates/ui/src/capture.rs` only where needed; no hosting-app restart. verify: normal/narrow widths, light/dark, real runtime chain and screenshot/output evidence per changed behavior.
- [ ] 9.4 Run final independent Spec, Standards/privacy and Reality review on the integrated diff and fill the conformance table below from actual evidence. Return any unmet requirement to the same durable ticket as REVISE. files: final diff and evidence records. verify: no inferred pass from terminal status, build-only result or static demo; publication gates stay not yet due unless publication is separately authorized.
- [ ] 9.5 After smoke proves behavior, update only affected DOX/Test Coverage Matrix and current architecture/design/functional/changelog contracts; remove throwaway probes. Do not mark unchecked implementation tasks complete or archive on planning validation alone. files: affected domain instructions, `ARCHITECTURE.md`, `DESIGN.md`, `FUNCTIONAL-BASELINE.html`, `fork_changelog.md`, this change's tasks/evidence. verify: documented fidelity/limits match native evidence; unrelated checkout work is untouched.

## Conformance Ledger

A future ACCEPT requires every row to carry actual evidence and the reviewed tests that defend it. Pending rows below are deliberately not implementation claims.

| Requirement | Main phase | Acceptance focus | Evidence |
|---|---|---|---|
| R1 | P1 | Start/result share outcome and sources; event identity retained | pending |
| R2 | P1/P2 | Scoped IDs, repeats, replay, cross-page operation | pending |
| R3 | P1 | Missing result versus success/error/late completion | pending |
| R4 | P2 | Consumed Turns, model Steps, independent Calls folds | pending |
| R5 | P3 | Instants/intervals, mixed legacy and timing provenance | pending |
| R6 | P4 | Context versus consumption, measured zero, dedup/cache | pending |
| R7 | P6 | Observed schema, snapshot precision and unavailable source | pending |
| R8 | P5/P6 | Opted-in original arguments/results, sanitized default | pending |
| R9 | P5 | Next-run Chat/profile consent and restart clearing | pending |
| R10 | P5/P6 | No diagnostic body in public/normalized/sync/export paths | pending |
| R11 | P5/P6 | Source limits/errors/ownership and late-response invalidation | pending |
| R12 | P7 | Recoverable history only, no journal rewrite/reset | pending |
| R13 | P1/P7/P8 | Live/reopen/revision/page convergence, selection/scroll | pending |
| R14 | P5/P7 | Bounded fail-open capture and semantic retention | pending |
