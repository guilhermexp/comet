## Why

The audited OMP Trajectory contains completed results for all 34 unique tools, while their 38 call records still appear Running; it also loses observed hierarchy, usable timing and context occupancy. Static schema descriptions and normalized-only raw arguments prevent the inspector from answering what actually ran.

## What Changes

- Correlate each scoped tool operation across immutable events and history pages, preserving the selected event while exposing its actual outcome, payload and result.
- Capture observed input/model boundaries; make Calls folding independent of model content and Turn folding.
- Render observed instants and intervals with timing provenance; isolate sequence-only history instead of invalidating all Recorded geometry.
- Preserve authoritative model usage separately from context occupancy, including unknown/partial versus measured zero and replay-safe aggregation.
- Capture runtime schema snapshots with provenance rather than claiming static normalized type descriptions are effective schemas.
- Add explicit next-run complete diagnostic capture for one Chat/profile, with a separate bounded local source and authorized ephemeral Reveal. Semantic sanitized capture remains always on.
- Enrich recoverable history idempotently without resetting Trajectory or rewriting recovery journals; unavailable historical schemas/arguments remain unavailable.
- Require end-to-end adapter/store/watch/projection coverage and native GPUI evidence, not only fabricated inspector fixtures.

## Capabilities

### New Capabilities

None; diagnostic capture belongs to the existing Trajectory capability.

### Modified Capabilities

- `chat-trajectory-preview`: Correlated operation inspection, observed hierarchy/timing/usage/schema fidelity, opt-in complete source capture, bounded private retention and non-destructive historical enrichment.

## Impact

- `crates/proto`: Shared operation derivation and additive diagnostics/provenance contracts.
- `crates/harness`: OMP source extraction and internal metadata envelope before normalization loses fields; migrate internal stream consumers without leaking raw data through AgentEvent.
- `crates/engine`: Indexed operation lookup, capture metadata, separate diagnostic writer/source, authorized reads, retention/deletion and versioned historical enrichment.
- `crates/rpc`: Local-only operation query and capture controls; additive raw-source/version availability support with ownership and forwarding checks.
- `crates/ui/src/trajectory`: Coherent inspector, folds, timing, usage/schema display, explicit opt-in and ephemeral Reveal.
- No new dependency, synchronized document schema, Worker trajectory, provider billing/usage integration, raw export, or recovery-journal expansion is planned. Existing normalized transcript and journal behavior remains unchanged.

Planning source: `docs/plans/2026-09-06-0304-fix-trajectory-observability-fidelity-plan.md` (R1–R14, U1–U8). Privacy decision: `docs/adr/0005-complete-trajectory-capture-is-opt-in.md`, complementing ADR 0004. This proposal does not authorize implementation, commit, publication or restart of the hosting Comet process.
