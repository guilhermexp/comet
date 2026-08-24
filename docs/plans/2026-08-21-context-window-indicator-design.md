# Context Window Indicator Design

## Job

Give the user a persistent, trustworthy view of the active chat's context pressure without opening settings or asking the agent. The indicator belongs inside the composer because that is where the user decides whether to send, compact, or start fresh.

## Data contract

- Extend normalized usage snapshots with optional `context_tokens` and `context_window` fields.
- Keep context usage separate from cumulative billing and per-turn input/output totals.
- Preserve the latest valid snapshot on the chat's live `Session`, so the existing sessions watch transports it to the UI.
- Use runtime truth only:
  - Codex: `thread/tokenUsage/updated` current/last snapshot plus `modelContextWindow`.
  - Claude Code: the result's real input/cache/output usage with the selected 200K/1M window.
  - OMP: `get_state.contextUsage.tokens/contextWindow` after each settled turn.
  - ACP/Cursor: consume context fields when their wire provides them; otherwise remain unknown.
- Never sum historical turns. Compaction, cache reuse, and provider-side pruning make cumulative totals invalid as current-context measurements.

## Component

- A compact 18px progress ring sits inside both compact and expanded composer action clusters, immediately left of the attachment control.
- Before the first trustworthy snapshot, it renders a quiet neutral ring. Tooltip: `Janela de contexto` / `Aguardando primeiro turno`.
- With data, the arc shows the clamped used fraction. Tooltip shows:
  - `Janela de contexto`
  - `NN% usado (MM% restante)`
  - `<usado> / <janela> tokens`
- Reuse existing usage thresholds: normal below 80%, warning at 80%, danger at 95%.
- The ring is informational, keyboard-focusable, and has the same tooltip content as its accessibility label. It performs no action on click.

## Boundaries

- Do not alter provider account quotas, billing usage, model selection, compact behavior, or composer geometry beyond the control's reserved width.
- Do not fabricate a model limit when the runtime has not supplied or deterministically selected one.
- Keep the snapshot last-known per chat: only a newer runtime measurement replaces it. Superseded by `openspec/specs/context-usage-continuity/spec.md`, which owns this contract.

## Validation

- Protocol and serde compatibility tests for old and new usage/session payloads.
- Runtime normalization tests for Codex, Claude, and OMP.
- Engine session-watch tests for snapshot update and retention behavior.
- Pure formatting, threshold, and neutral-state UI tests.
- Real GPUI app inspection at the standard and narrow layouts, including tooltip and high-usage fixtures.

