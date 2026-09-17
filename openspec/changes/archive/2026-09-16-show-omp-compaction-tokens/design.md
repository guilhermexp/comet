## Context

`run_session` directly emits Done for `agentInvoked:false`, bypassing `finish_agent_end`, which requests `get_state` and emits ContextUsage. The UI already renders a before/after marker when context usage decreases. The installed OMP binary exposes `contextUsage` through `get_state`.

## Goals / Non-Goals

Refresh usage before local command completion without duplicating completion logic. Do not introduce new wire types, rewrite old markers or change automatic compaction presentation.

## Decisions

Reuse `finish_agent_end(Complete)` after draining local command output. Keep usage fetch best-effort and bounded by the existing RPC timeout. Test the real subprocess adapter against synthetic local-command responses, including missing or failed state reads.

## Risks / Trade-offs

Successful local commands gain one bounded state request, as regular turns already do. Runtimes without context usage retain the generic marker. Existing UI timing and its pre-compaction snapshot are unchanged.

## Migration Plan

No migration; existing transcript markers remain unchanged.

## Open Questions

None.
