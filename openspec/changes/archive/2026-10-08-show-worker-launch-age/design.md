# Design

## Context

`ChatWorkerRow` already carries original creation and update timestamps. The render has a compact model/token subtitle and a command-only fallback. `format_time_ago` supplies sidebar units. Details already owns a 30-second usage clock, but it only notifies with a usage snapshot.

## Goals / Non-Goals

Keep launch age bound to the Worker identity across output and lifecycle changes, with no new timer or timestamp schema. Background execution collection is independent and remains in `improve-workers-background-observability`.

## Decisions

- Project an optional age using original creation milliseconds and an injected wall clock. Zero/out-of-range timestamps omit the label; future time follows the existing formatter's `now` clamp. Heartbeat and settle timestamps would measure a different event, so they are excluded.
- Keep the current subtitle and add a fixed-width age beside its token total; the command-only variant uses the same flexible text/fixed age structure. A tooltip identifies the age as time since launch and gives the creation time.
- Extend the existing 30-second tick to notify when visible Worker rows need an age refresh, even without provider usage. Reuse the current entity/task lifetime; no per-row timer or animation clock.

## Risks / Trade-offs

- Labels can lag a unit boundary by at most the existing clock interval; this avoids adding a faster render loop for minute/hour labels.
- The gpui row has no automated render harness; native observation verifies placement, narrow layout and fallback while pure tests cover origin/missing/future timestamps.
