# Design

## Context

See proposal.md and the `workers-host-bridge` delta. `crates/harness/src/claude/mod.rs` protects every open tool by sending the incoming message at `priority: next`; `claude/wire.rs` documents that `now` cancels an in-flight MCP call while keeping the CLI alive. `omp/mod.rs` already has request-scoped cancellation and exactly-once host-tool result delivery, but only applies it to Worker notices. The controller already supports request cancellation without touching the Worker.

## Goals / Non-Goals

**Goals:** Release only the read-only status wait, preserve unrelated tools, and prove early message consumption rather than mere stdin/mailbox writes.

**Non-Goals:** No persisted markers, retry system, timeout reduction, provider-wide interruption, global Stop, Worker hooks or provider setting changes. Explicitly queued messages and pending-update gates retain their existing meaning.

## Decisions

1. Expose the engine's existing authoritative `has_pending_steers` ledger through a small read-only, non-forwardable `WorkerWaitState` RPC (`{chatId}` → `{pendingMessage}`). The ledger lasts from acceptance until provider consumption, so a message accepted before the wait starts still releases it. Do not create a second counter or notification state.
2. A parent-bound controller wait uses the already-granted engine endpoint to check this state on its existing 250ms polling loop, reusing one bounded connection/runtime per wait. With a pending message, return a normal tool result (`matched: false`, incoming-message reason and current Worker snapshot), allowing native Claude, Pi, Codex and ACP tool boundaries to advance without any provider-wide cancel. Standalone controllers with no parent/endpoint retain their existing wait behavior. An explicitly supplied but unavailable endpoint is an observable error, never silently treated as an empty queue.
3. In OMP, generalize the existing request-scoped cancellation to all incoming steers, with a successful interrupted result before queued-steer delivery. Preserve natural-result precedence and the duplicate-result guard. This remains necessary for standalone host-tool bridges without an engine endpoint.
4. Keep the native adapters' ordinary-tool protection unchanged. A wait alongside another tool can finish independently without aborting that tool. Reject the native `priority: now`/`turn/interrupt` alternative because it cancels an entire tool step rather than the single status wait.
5. Keep tests at actual engine/RPC and controller boundaries, plus OMP subprocess regressions. Use a disposable smoke with the real engine, WebSocket RPC, controller process and shell Worker; its controlled parent proves wait release, continued Worker execution and single consumption, not the native app UI. Never launch a second app or modify user provider configuration.

## Risks / Trade-offs

- IPC polling overhead → reuse one bounded connection/runtime per active wait and reuse its existing polling cadence, not a new global poller.
- Completion and message race → check real Worker match/completion evidence first; a pending message must not overwrite a natural result.
- Startup/consumption race → query the existing pending-steer ledger, not a one-shot wakeup which can be missed before the wait registers.
- Native consumption semantics → unchanged native adapters rely on their existing tool-boundary behavior. The disposable engine/controller smoke is not proof of native Claude/Codex/Pi UI delivery; report that limit rather than claiming those surfaces were exercised.

## Migration Plan

Rebuild the local app after focused verification. No persistent data migration or release is part of this request. Rollback is the source change only.
