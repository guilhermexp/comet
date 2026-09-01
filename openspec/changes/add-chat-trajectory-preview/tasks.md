# Tasks

## Fasing

| Fase | U-IDs | Seções | Depends on | Audit state | Audited commit | Entrega | UAT mode |
|---|---|---|---|---|---|---|---|
| F1 | U1, U2 | §1 | — | gaps_found | 6268bb9843eec2add5e86f25015565dfed79c664 | Typed contracts plus fail-open local store, capture, legacy projection, and deletion lifecycle | none — automated |
| F2 | U3 | §2 | F1 | pending | — | Local-only coherent snapshot/delta watch and bounded owner-checked raw reveal | none — automated |
| F3 | U4, U5 | §3 | F2 | pending | — | Complete GPUI Trajectory surface and one-per-Chat titlebar/right-pane lifecycle | artifact-driven |
| F4 | U6 | §4 | F3 | pending | — | Native evidence across live, legacy, privacy, degraded, responsive, and theme states; DOX closure | human-driven |

## Boundary Map

| Phase | Owns | Must not change |
|---|---|---|
| F1 | `crates/proto`, Trajectory store/capture and Chat lifecycle in `crates/engine` | RPC/UI behavior, synchronized document schemas, export, dependencies |
| F2 | `crates/rpc` Trajectory protocol and `crates/engine` Trajectory handlers/raw adapter | GPUI shell/surface, edge forwarding, synchronized state |
| F3 | `crates/ui/src/trajectory`, capture fixtures, titlebar and right-pane integration | engine storage semantics, global Details, Capture behavior, dependencies |
| F4 | Remaining deterministic fixtures, native QA, capability inventories, DOX matrices | New product behavior, pruning, cross-device sync, CLI Worker trajectories |

## 1. Foundation — Contracts, Store, and Capture

**must_haves:** capture remains active with zero viewers and survives restart; Trajectory failures never fail or block a Chat run, Run Journal, or Chat Transcript; every visible record has stable lane/order/run identity and missing timing remains unavailable; SQLite stores sanitized values and no raw body; archive retains rows while every authoritative Chat deletion path removes them.

- [x] 1.1 Add test-first typed Trajectory contracts and pure projections for stable identity, Input/Model/Tools classification, error precedence, run/turn/step grouping, recorded versus sequence-only timing, sanitized fields, partial-to-final replacement, and idempotent deltas; export the module. files: `crates/proto/src/trajectory.rs` (new), `crates/proto/src/lib.rs`. verify: `cargo test -p zeron-proto trajectory`.
- [x] 1.2 Add temporary-profile tests and implement the versioned SQLite store with WAL, a bounded nonblocking queue, one ordered writer, independent readers, batched semantic writes, watermarks, paging, diagnostics, reopen, and explicit degraded intervals. files: `crates/engine/src/trajectory_store.rs` (new), `crates/engine/src/lib.rs`. verify: `cargo test -p zeron-engine trajectory_store`.
- [x] 1.3 Add capture tests and connect normalized publish-time projection without token-level rows, coalescing partial text/reasoning on the existing presentation cadence while preserving final identity and allowing Run Journal/event publication to continue on store failure or queue saturation. files: `crates/engine/src/sessions.rs`, `crates/engine/src/trajectory_store.rs`. verify: `cargo test -p zeron-engine trajectory_capture`.
- [x] 1.4 Add idempotent legacy projection and lifecycle coverage for timestamp-free sequence mode, valid-prefix corrupt-tail import, interrupted/unsettled recovery, no invented values, archive retention, and local/synchronized/Space-cascade deletion observed through the authoritative workspace Chat set. files: `crates/engine/src/trajectory_store.rs`, `crates/engine/src/workspace_host.rs`. verify: `cargo test -p zeron-engine trajectory_legacy`.
- [x] 1.5 Record final proto/engine ownership and Test Coverage Matrix tiers, then run the complete focused foundation suites. files: `crates/proto/AGENTS.md`, `crates/engine/AGENTS.md`. verify: `cargo test -p zeron-proto && cargo test -p zeron-engine trajectory`.

## 2. Transport — Local Snapshot, Deltas, and Reveal

**must_haves:** a watcher receives coherent history and live records exactly once across the handoff; closing a watch affects delivery only; Trajectory RPCs cannot be forwarded to another device; raw reveal returns one owner-checked local field without persisting or synchronizing it; large/corrupt/missing raw sources cannot starve async RPC or fabricate a value.

- [ ] 2.1 Add test-first bounded snapshot/page, watermark, delta, degraded/terminal, unavailable, and reveal wire contracts with protocol compatibility coverage. files: `crates/rpc/src/lib.rs`. verify: `cargo test -p zeron-rpc trajectory`.
- [ ] 2.2 Add engine handler tests and implement atomic snapshot-watermark-delta watch, bounded history frames, stable deduplication, explicit regap/resnapshot, cancellation, deletion terminal state, and degraded-store behavior over independent readers. files: `crates/engine/src/rpc.rs`, `crates/engine/src/trajectory_store.rs`. verify: `cargo test -p zeron-engine trajectory_watch`.
- [ ] 2.3 Add privacy/security tests and implement local-only raw reveal with Chat/source/parent/call/field ownership checks, versioned Run Journal lookup in `spawn_blocking`, line-size guards, responsiveness coverage, and typed unavailable results for foreign, stale, corrupt, deleted, or mismatched sources. files: `crates/engine/src/rpc.rs`, `crates/engine/src/run_journal.rs`, `crates/rpc/src/lib.rs`. verify: `cargo test -p zeron-engine trajectory_reveal`.
- [ ] 2.4 Record final RPC/engine protocol ownership and Test Coverage Matrix tiers, then run the complete focused transport suites. files: `crates/rpc/AGENTS.md`, `crates/engine/AGENTS.md`. verify: `cargo test -p zeron-rpc && cargo test -p zeron-engine trajectory_rpc`.

## 3. Surface — GPUI Trajectory and Shell Integration

**must_haves:** users can navigate fixed Input/Model/Tools lanes, hierarchical virtual ledger, Duration/Turns/Calls/Search, synchronized selection, and internal inspector; large/live histories preserve semantic selection and viewport unless explicitly following the live edge; sanitized Payload/Result is the default and raw state clears with view lifecycle; one titlebar control beside Capture opens/focuses one surface per Chat while closing presentation never controls capture.

- [ ] 3.1 Add pure tests and implement the view model for run/turn/step/event hierarchy, independent Turns/Calls folds, search/range dimming, selection, timing mode, stable row identity, partial/final replacement, viewport anchors, live-follow suspension, and ephemeral reveal clearing. files: `crates/ui/src/trajectory/model.rs` (new), `crates/ui/src/trajectory/mod.rs` (new), `crates/ui/src/lib.rs`. verify: `cargo test -p zeron-ui trajectory_model`.
- [ ] 3.2 Add pure geometry/virtualization tests and implement fixed three-lane timeline plus hierarchical ledger with sequence/recorded-duration modes, valid-only TTFT/decoding, error override, offscreen selection targeting, historical prepend anchoring, and live append stability. files: `crates/ui/src/trajectory/timeline.rs` (new), `crates/ui/src/trajectory/ledger.rs` (new). verify: `cargo test -p zeron-ui trajectory_timeline`.
- [ ] 3.3 Add inspector/toolbar/view tests and implement Summary/Payload/Result/Schema/Timing, explicit raw reveal, Duration/Turns/Calls/Search, wide split versus narrow internal detail, explicit failure/unavailable states, and a surface-owned RPC watch task with `AppState` only supplying engine access. files: `crates/ui/src/trajectory/inspector.rs` (new), `crates/ui/src/trajectory/toolbar.rs` (new), `crates/ui/src/trajectory/view.rs` (new). verify: `cargo test -p zeron-ui trajectory_view`.
- [ ] 3.4 Add capture-gating tests and deterministic `ZERON_UI_CAPTURE=1` core fixtures for populated multi-run, legacy sequence-only, selected tool error, sanitized/unavailable raw, and narrow states; prove every knob is inert in normal startup. files: `crates/ui/src/capture.rs`. verify: `cargo test -p zeron-ui trajectory_capture`.
- [ ] 3.5 Add shell lifecycle tests and integrate `RightSurface::Trajectory`, per-panel registration, open/focus/close, one surface per Chat, per-Chat state isolation, archive/delete behavior, existing resize/reorder/expand/takeover/fallback behavior, and the accessible content-sized titlebar control directly beside Capture. files: `crates/ui/src/shell.rs`, `crates/ui/src/shell/tabs.rs`, `crates/ui/src/icons.rs`. verify: `cargo test -p zeron-ui trajectory_shell`.
- [ ] 3.6 Record final UI ownership and Test Coverage Matrix tiers, then run the complete focused UI trajectory suite and formatting check. files: `crates/ui/AGENTS.md`. verify: `cargo test -p zeron-ui trajectory && cargo fmt --all --check`.

## 4. Reality — Native QA and Closure

**must_haves:** the native app shows the confirmed titlebar/right-pane information hierarchy at standard and narrow widths; live capture and close/reopen behavior are observed rather than inferred from tests; dark/light semantic states and keyboard focus remain distinguishable; every origin acceptance example has recorded behavioral or native visual evidence; final docs name real owners and no unrequested capability enters scope.

- [ ] 4.1 Complete deterministic fixtures for live partial/final replacement, degraded storage, multiple Chats, and theme-sensitive states, changing the demo script only when supported inputs cannot seed the required real flow. files: `crates/ui/src/capture.rs`, `scripts/dev-demo.sh`. verify: `cargo test -p zeron-ui trajectory_capture`.
- [ ] 4.2 Launch the real native GPUI app and exercise titlebar activation, one-surface focus, history/live convergence, Duration/Turns/Calls/Search, timeline-to-ledger selection, inspector views, reveal/clear, close/reopen continuity, per-Chat isolation, large-history scrolling, live-edge suspension, and legacy/degraded/unavailable states at standard and narrow widths. files: `crates/ui/src/trajectory`, `crates/ui/src/shell.rs`, `crates/ui/src/shell/tabs.rs`. verify: `cargo test -p zeron-ui trajectory`.
- [ ] 4.3 Observe dark and light themes plus keyboard/focus/contrast behavior, reconcile final DOX/Test Coverage Matrices, and update capability inventories where this titlebar/right-pane surface is represented. files: `crates/proto/AGENTS.md`, `crates/engine/AGENTS.md`, `crates/rpc/AGENTS.md`, `crates/ui/AGENTS.md`, `docs/PARITY.md`, `FUNCTIONAL-BASELINE.html`. verify: `grep -q "Trajectory" docs/PARITY.md && grep -q "Trajectory" FUNCTIONAL-BASELINE.html`.
- [ ] 4.4 Run final affected-crate tests and formatting once, preserve independent Spec and Reality evidence, and leave Standards & Security as not-yet-due until an enforceable push gate exists; do not push, merge, archive, or release from the worker. files: `crates/proto`, `crates/engine`, `crates/rpc`, `crates/ui`. verify: `cargo fmt --all --check && cargo test -p zeron-proto -p zeron-engine -p zeron-rpc -p zeron-ui`.
