# Context usage

Every selected conversation has a context ring beneath its composer, including
project-less and remote conversations. Hovering shows measured tokens and
remaining capacity. The ring turns amber at 75% and red at 90%; its drawing clamps
at a full circle while the label preserves over-capacity measurements. A dash
means the harness has not reported enough data to calculate a percentage. Measured
zero is displayed as 0%.

The host normalizes context occupancy separately from billing `Usage` events:

| Harness | Measurement |
| --- | --- |
| Claude Code | Latest parent assistant message's input plus cache-read/cache-creation tokens; capacity from that model's result metadata. Aggregate result billing and child agents are excluded. |
| Codex | Latest model call (`tokenUsage.last`), with `modelContextWindow`; never the cumulative thread total. |
| OpenCode | Latest parent assistant message's total, or input/output/cache counts; capacity from the advertised provider/model catalog. Empty in-progress placeholders do not clear a measurement. |
| ACP (Devin, Grok, Hermes, pi) | `usage_update.used` and advertised capacity when the agent reports them. |
| Cursor | The pinned SDK exposes billed per-turn counts, not context occupancy. The shared control shows unavailable. |
| Mock / older hosts | Unavailable until a context snapshot is supplied. |

`AgentEvent::ContextUsage` updates one atomic `meta.contextUsage` value in the
session document. Partial measurements preserve known fields; a zero capacity is
ignored. New non-resumed runs clear old usage. Post-turn updates can refresh the
snapshot without reopening a completed turn, and subagent events cannot change
the parent meter.

The existing document sync carries the value through the relay and local storage.
`WatchDocMessages` includes a typed `TranscriptUpdate` envelope with the snapshot,
including context-only commits and the opening reset. Old readers ignore the
additive field; old hosts decode as unavailable. Thin-document rebuilding also
preserves the snapshot. UI rendering reads the in-memory state, with no polling,
provider calls, filesystem access, or new remote endpoint.

Validation covers adapter normalization, compaction to zero, cache accounting,
subagent isolation, snapshot and incremental import, thin rebuilds, context-only
watch updates, reconnects, and old/new watch compatibility. Screenshots use the
native desktop attached to a viewing engine; a separate host sends deterministic
Claude protocol fixtures through a local relay. The measurements are test data.

## Session Context details

Details → Session keeps the compact Context indicator. Its inline chevron opens
an optional native budget with used/capacity counts, category percentages and
independent MCP-tool and memory-file lists. Expansion belongs to that Details
context. Deferred tools have no active-budget percentage. Nested attribution is
included in its parent category, not added to the used total or the segmented
bar. Unknown cells display a dash; measured zero remains zero. Over-capacity
counts remain visible while bar geometry stays within its width.

This is separate from aggregate `ContextUsage` continuity and from Context
sources (the project's skill catalog). Category estimates are not billing.
Observation time is when the host received an inspection, whereas provider
measurement time is shown only if the runtime actually supplies it. A retained
snapshot is labeled stale after a failed refresh or an ended/replaced runtime.

### Optional host protocol

`GetChatContextBreakdown` accepts `{chatId, targetDeviceId?}` and forwards to the
selected Chat's host. The reply is a typed `ContextBreakdown` in
`zeron-proto::context_breakdown`: host/Chat/runtime/native-session/model identity,
source/schema, observation and native measurement time, revision/epoch, optional
totals, category cells and metadata-only lists. Availability is explicit
(`available`, `partial`, `unsupported`, `notApplicable`, `error`). Loading is UI
state. Missing lists differ from available empty lists.

The host resolves an existing runtime, coalesces concurrent reads and validates
its identity again before accepting a reply. It keeps only the latest snapshot
in memory per Chat. Detailed reports never enter Loro, Chat Transcript or a
persistent diagnostic history. No runtime is started to answer this method;
there is no local fallback for another host's Chat. Older engines may return
UnknownMethod, which means details are unavailable.

The harness uses a narrow request/reply mailbox in `TurnControl`, independent of
steering, stopping, questions and configuration. Requests expire, and runtime exit
fails outstanding readers. Opening, explicit Refresh and observed turn/lifecycle
or local compaction transitions while expanded can request a snapshot. Late usage after a turn settles also refreshes; active token counters do not. There is no timer,
per-token refresh, diagnostic prompt or provider token-count request.

The displayed percentage uses the same inspected totals as the expanded budget.
The native model and observation time remain visible. Runtime identity is checked
on each inspection; an idle-only retirement that does not change the watched
Session boundary is observed on the next Refresh, rather than by polling.

| Native runtime | Inspection and compatibility |
| --- | --- |
| OMP | `get_context_breakdown`, report schema version 1 from the coordinated OMP contract. Its five native categories remain intact. Memory/context files and MCP instructions are attribution inside `systemContext`, never additional active budget. Old OMP's Unknown command produces an explicit update-required state, cached for that process. Unknown schemas remain unsupported. No `/context` prompt or prompt reconstruction is used. |
| Claude Code | Verified binary version ≥2.1.257 only: stream-json `control_request` with `subtype:"get_context_usage", detail:"summary"`. Summary is approximate. Unknown/older versions are unsupported without issuing the control. Never omit detail or fall back to full counting. `apiUsage:null` is absent measurement, and receipt time is not provider measurement time. |
| Codex/other runtimes | Aggregate Context continues to work; native detail is unsupported unless its adapter supplies a read-only inspection source. |

Claude 2.1.318's summary control redacts memory-file metadata and MCP instruction
items. An empty memory array from this control is unavailable attribution, not a
measured absence of files. Native `isLoaded:false` is preserved independently of
deferred-tool accounting. The installed no-prompt smoke verifies the summary
control, not populated item lists.

OMP v1 preserves its complete native identity (session, provider/model, window,
compaction epoch, context revision, prompt digest and message count), lifecycle
flags and native capture/provider times. `hasLocalChanges:null` means fingerprints
cannot establish equality, even if token counts match. Memory/context files and
MCP instructions are nested under System context. Its fragment residual is signed
because BPE boundary estimates need not sum exactly; it is never redistributed.
Active MCP schemas stay nested in System tools. Model policy omission, xdev
names outside the active roster and deferred names in the last matching request
remain different mechanisms. Historical deferred metadata retains provider
`messageIndex` and current `transcriptIndex`, carries no active percentage, and
makes no current-activation claim. The native declared-roster estimate limitation
remains explicit.

The coordinated source-built loopback smoke uses the actual OMP transport and
normalizer for an existing idle process (zero requests/messages), then a separate
serial launcher for the real Engine/RPC path: first prompt 1 request, repeated
active inspections still 1, completion still 1, next ordinary prompt 2. The host
returns unavailability before a runtime exists; it never creates an OMP process
for inspection. The generic explicit idle-start API currently belongs to Codex.
This proof does not install a runtime or launch the dev app.

## Native turn timing

Turn stats uses native model time independently of tools and elapsed wall time.
OMP assistant `duration` values are floating-point milliseconds. Each finite,
nonnegative, representable contribution is rounded once, then summed. Zero is a
measurement; missing/invalid timing is not. A defaulted
`TurnMetrics.modelTimingIncomplete` flag marks any model-bearing contribution
whose timing is unavailable. A partial native sum never becomes complete model
time. Auxiliary cache/cost-only metrics do not invalidate complete timing.

Claude retains its native `duration_api_ms` semantics. Runtimes without native
timing display a dash for Model time and Response rate. Response uses output
(including reasoning, as reported) divided by complete native model seconds.
Whole turn continues to use elapsed wall time; tool overlap and child exclusion
are unchanged. There is no wall-time-minus-tools fallback in either the engine
or UI. A valid zero model duration has no divisible response rate.

A read-only replay of the recorded 18-request OMP turn retains 13,824 output
tokens, 38 fresh input tokens, 5,017,335 cache-read tokens, 52,343 cache-write tokens
and $1.698843 cost. Its native duration is about 143.6 seconds and its native
response rate about 96.3 tok/s. Private conversation content is not a fixture.

Actual installed-surface appearance is not proven by the safe GPUI harness.
The project's provider-isolation restriction prohibits launching a dev Comet or
dev-demo for this verification.
