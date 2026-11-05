# Design

## Context

See `proposal.md`. The native OMP JSONL carries floating-point `AssistantMessage.duration` milliseconds on `message_end`; Comet currently sets `TurnMetrics.model_ms` to `None`. `TurnAccumulator::finish` substitutes wall time minus tool spans, and the Session card substitutes duration again. Neither is a native model-time measurement.

The compact Context row uses `ContextUsage`, an aggregate snapshot with continuity semantics. `Context sources` is a skill catalog, not token attribution. Neither should be repurposed as a detailed budget. Installed OMP v18.16.0 rejects `get_context_breakdown`; its coordinated source change is `oh-my-pi/openspec/changes/expose-session-context-breakdown/`.

Claude Code 2.1.318's matching primary SDK documents `get_context_usage` with `detail:"summary"` (last usage plus local estimates, no token-count calls). Default/full detail performs provider token-count requests. The existing adapter discards successful control-response payloads. This contract is documented, not yet smoke-proven against the installed binary.

## Goals / Non-Goals

**Goals:** honest native turn timing; inline, accessible, context-scoped details; native host inspection without a model request; faithful OMP and Claude category semantics, partial availability and freshness.

**Non-Goals:** subscription billing, dashboard ingestion, new worker counters, runtime polling, Codex runtime changes, unrelated Session redesign, publishing/upgrading either installed executable in the planning step.

## Decisions

### D1 — Native timing only, with complete coverage

Parse OMP `duration` as finite nonnegative floating-point milliseconds, round once per contribution to checked `u64`, and populate existing `TurnMetrics.model_ms`. Zero is measured zero; missing, negative, nonfinite or unrepresentable is unavailable. Never parse via `as_u64`, which loses real native fractional values.

Add an explicit defaulted `model_timing_incomplete` marker to `TurnMetrics` for a model-bearing contribution with unavailable timing. Auxiliary cache/cost-only metrics do not mark a missing request. Migrate all constructors and consumers. Accumulate measured durations and a sticky incomplete flag, reset at every actual turn boundary. The published `TurnStats.model_ms` is present only if native timing exists and no covered request is missing; remove the wall-minus-tools fallback. Keep Claude's complete native `duration_api_ms` semantics and Codex's unavailable timing.

The UI uses measured model time for Response rate and Model time, otherwise `—`; no second fallback. Response is output tokens (including reasoning, as currently reported) divided by native model seconds, not a new post-TTFT metric. Whole turn continues to use actual turn wall duration. Preserve usage/cache/cost accounting, tool overlap union and child exclusion. For the recorded example: 13,824 output / 143.607892s is approximately 96.3 tok/s, not 106.

### D2 — Separate optional context snapshot

Keep aggregate `ContextUsage` and persisted `ChatContextUsage` semantics unchanged. Define a typed, optional `ContextBreakdown` response with host/chat/native-session/model identity, schema/source, observation time, native measurement time when known, context epoch/revision, totals, category cells and safe item lists. Availability is explicit: available, partial, unsupported, not-applicable or error; loading is local UI state. Missing token cells and unavailable lists are not zero or empty arrays.

Categories retain stable IDs, label, kind (`used`, `buffer`, `free`, `deferred`) and estimate/provenance. Preserve unknown native categories rather than dropping their budget. Primary active categories plus reserve/free drive the bar; nested attribution is not another contribution. The host retains only the latest snapshot per runtime identity in memory, never full diagnostic bodies in synced CRDT documents. Cached data from an ended/replaced runtime may remain visibly stale, but cannot become current for another session/model. No new persistent history store is needed.

### D3 — Host-scoped read-only path, not steering

Add a narrowly scoped context-inspection request/reply mailbox to `TurnControl`; it is not a `SteerMessage`, user prompt, stop or configuration control. Drivers service it through their existing live loop, including between turns. Correlate and expire requests; fail outstanding waiters on runtime exit without changing config certainty or turn state.

Expose a typed forwardable `GetChatContextBreakdown` RPC to the chat's host, with the same target-device routing model as existing forwarded metadata getters. Engine resolves the selected chat's current runtime, coalesces concurrent identical inspections and validates the runtime identity before accepting the result. If there is no live runtime, return the retained snapshot marked stale or explicit unavailability; do not spawn a runtime just for inspection. Do not read local data as a remote chat fallback.

The UI requests on opening the row, explicit refresh and a completed turn/compaction transition while expanded, not on a timer or every streaming token. An outstanding result is ignored if the selected chat/device/model/context has changed. Collapse stops UI refresh triggers. No general-purpose diagnostic RPC or transcript retrieval is introduced.

### D4 — Native adapters and compatibility

**OMP:** use `OmpProcess::request({type:"get_context_breakdown"})`. Consume report schema v1 defined in the coordinated OMP design. Unknown command means this runtime needs the OMP update; cache that capability result for the process lifetime and do not retry on every render. Unsupported schemas remain explicit. No `/context` prompt, text parsing, hooks, prompt rebuild or provider call.

Map OMP's five primary categories directly. `systemContext` contains context files/memory and MCP instructions: show attributed rows nested under that aggregate, clearly marked as included and approximate at fragment boundaries. Do not add them to the bar or used total again. Distinguish model-omitted schemas, xdev discoverable tools and historical provider defer-loading; none becomes a fictitious current active percentage. Preserve unavailable reasons for overrides/custom prompts.

**Claude:** send stream-json `control_request` with `subtype:"get_context_usage", detail:"summary"`. Require verified binary version >=2.1.257 (the documented introduction); older or unparseable versions are unsupported because ignoring `detail` could execute full counting. Never fall back to full. Extend `ControlResponseBody` to retain successful response payloads and route correlated responses to context waiters before normal event normalization. Summary categories are approximate, observation time is local receipt time, and native measurement time is unknown unless independently known. `apiUsage:null` is not measured. Never stamp inspection time as the last provider measurement.

**Codex/others:** preserve aggregate Context display; report unsupported breakdown unless a genuinely native read-only source is already provided. No changes to their runtimes and no invented category estimates.

### D5 — Inline UI and accessibility

In `crates/ui/src/details_sidebar/session_card.rs`, add a chevron/control to the existing Context row, keep the compact default and reveal details below the bar inside Session. Reuse per-context expansion preferences in `view.rs` with a distinct `:context:expanded` marker; no new card/modal. Use the existing design tokens, spacing, type and icon patterns from DESIGN.md and Impeccable.

Expanded content: used/capacity, a segmented capacity bar, native category counts and capacity percentages; reserve/free separately; deferred rows without active percentages; independent MCP-tool and memory-file disclosure lists with item count, bounded scrolling and metadata only. Preserve categories that are not applicable or unavailable with a concise explanation, not fake zeros. OMP-only system context can contain nested memory/MCP attribution, unlike Claude's flat native categories. Staleness, approximation, source and last known measurement are visible. Over-capacity values remain honest in text while geometry is safely bounded. Zero/unknown capacity must not divide by zero.

Accessible names and expanded state must be exposed; support keyboard activation and focus. Interaction must not toggle Session, Project or unrelated source sections.

## Verification

- Red/green focused normalizer and accumulator regressions for fractional timing, zero, invalid/missing/partial data, turn resets and unchanged usage/cost/child boundaries. Keep behavior assertions, not source-text tests.
- Real OMP JSONL replay of the observed turn: native model duration approximately 143.6s and Response approximately 96.3 tok/s; aggregate tokens/cache/cost remain unchanged.
- Adapter integrations exercise a control reply between turns and while active, old-OMP unknown-command, Claude version safety, correlation/timeout/exit, and the next normal prompt. Prove no prompt/turn/provider request was caused by inspection, not just that bytes were forwarded.
- Host routing and UI-state checks cover cross-chat/device results, late responses, stale/partial/unsupported states, nested non-additivity, zero capacity and isolated expansion state.
- Smoke the source-built OMP RPC with a controlled local provider, then consume its real v1 output through the Comet adapter/host path. Probe the installed Claude's summary control without generating a prompt if permitted; otherwise retain the documented limit explicitly and use the existing adapter integration harness.
- Project rules prohibit launching a dev Comet copy because provider configuration is not isolated. Use the existing safe UI harness for state/layout proof; do not claim installed-surface visual QA. Record actual-surface proof as notProven until an authorized installed build can be observed. Do not bypass the restriction with dev-demo.
- Canonical focused checks follow native project instructions; heavy Rust suite goes through box-test. Gate final commits with review-pair, use immediate recorded manual contingency for quota/service failures, and retain secrets scanning.

## Risks / Trade-offs

- OMP requires a coordinated source/runtime update; old installations still get corrected native timing, but explicitly unsupported context detail.
- Native local attribution is not provider-exact billing. Showing provenance is required to avoid another misleading stats panel.
- Local latest-only detail cache is intentionally not synced or durable: after a host restart details can be unavailable until the runtime is live, while aggregate Context continuity remains intact.
- An unknown Claude version cannot safely be probed by falling back to full. This is a deliberate no-cost safety boundary, not a hidden failure.
- Integration, commit destination and release/install destinations must be recorded explicitly before implementation; publication was postponed by the user and is not implied by this plan.
