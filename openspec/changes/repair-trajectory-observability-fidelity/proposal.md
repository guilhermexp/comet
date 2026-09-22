## Why

The 2026-09-22 audit confirmed the same fidelity failures on the user's current Chat: a snapshot contained 20,373 records with no turn/step identity, and 81 call records with matching results still appeared Running. The search field is not editable, long inspector content is clipped, and legacy import can persist unredacted preview text, so the feature needs functional UI repair as well as trustworthy operation, timing, usage and source data.

## What Changes

- Correlate each scoped tool operation across immutable events and history pages, preserving the selected event while exposing its actual outcome, payload and result.
- Capture observed input/model boundaries; make Calls folding independent of model content and Turn folding.
- Render observed instants and intervals with timing provenance; isolate sequence-only history instead of invalidating all Recorded geometry.
- Preserve authoritative model usage separately from context occupancy, including unknown/partial versus measured zero and replay-safe aggregation.
- Capture runtime schema snapshots with provenance rather than claiming static normalized type descriptions are effective schemas.
- Add explicit next-run complete diagnostic capture for one Chat/profile, with a separate bounded local source and authorized ephemeral Reveal. Semantic sanitized capture remains always on.
- Enrich recoverable history idempotently without resetting Trajectory or rewriting recovery journals; unavailable historical schemas/arguments remain unavailable.
- Require end-to-end adapter/store/watch/projection coverage and native GPUI evidence, not only fabricated inspector fixtures.
- Wire a real search input, technical-ID matching and consistent dimming across folded groups; make timeline selection reveal its required ledger path.
- Make inspector content scrollable, put sanitized action/target/outcome before technical metadata, and preserve fixed-height ledger rows.
- Correct legacy preview sanitization at import and read boundaries, and repair already-persisted previews without altering recovery journals.
- Keep explicit live-edge return available on a quiet stream and verify responsiveness with a synthetic 20,373-event history.

## Capabilities

### New Capabilities

None; diagnostic capture belongs to the existing Trajectory capability.

### Modified Capabilities

- `chat-trajectory-preview`: Correlated operation inspection, observed hierarchy/timing/usage/schema fidelity, functional search/folds/scroll/navigation, safe legacy previews, opt-in complete source capture, bounded private retention and non-destructive historical enrichment.

## Impact

- `crates/proto`: Shared operation derivation and additive diagnostics/provenance contracts.
- `crates/harness`: OMP source extraction and internal metadata envelope before normalization loses fields; migrate internal stream consumers without leaking raw data through AgentEvent.
- `crates/engine`: Indexed operation lookup, capture metadata, separate diagnostic writer/source, authorized reads, retention/deletion and versioned historical enrichment.
- `crates/rpc`: Local-only operation query and capture controls; additive raw-source/version availability support with ownership and forwarding checks.
- `crates/ui/src/trajectory`: Coherent inspector, folds, timing, usage/schema display, explicit opt-in and ephemeral Reveal.
- No new dependency, synchronized document schema, Worker trajectory, provider billing/usage integration, raw export, or recovery-journal expansion is planned. Existing normalized transcript and journal behavior remains unchanged.

Current execution plan: `docs/superpowers/plans/2026-09-22-trajectory-repair.md` (T01–T16, R1–R22). The 2026-09-06 plan is historical context; its R1–R14 retain their meaning. Privacy decision: `docs/adr/0005-complete-trajectory-capture-is-opt-in.md`, complementing ADR 0004. This revision plans the repairs requested by the user; it does not record implementation, commit, publication or restart of the hosting Comet process.
