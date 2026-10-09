# Design

## Context

The process-global OS wake broadcast is distinct from Chat catch-up commands and sibling-online hints. Registry and device-room host actors consume it only during backoff. Chat already has whole-session shutdown cancellation and retains abort ownership; registry lacks both patterns.

## Goals / Non-Goals

**Goals:** refresh stale active sockets after resume and make registry lifecycle cancellation safe without changing durable sync semantics.

**Non-Goals:** change wire formats, erase outbox entries, disable OMP extensions, suppress actual DNS/provider/quota errors, or claim that historical logs prove current defects already fixed in source.

## Decisions

- Select the OS wake receiver around active transport lifetimes, including dial/setup where needed. Reset backoff on a fresh wake and retain existing replay semantics. Sibling-online and consumer catch-up remain separate.
- Port Chat cancellation ownership to registry: await the handle by mutable reference and retain it until completion; select shutdown around dial and the entire session so blocked sends are covered.
- Preserve explicit host-offline cooldowns across local wake, matching existing token/online handling; fresh remote presence remains authoritative.
- Add minimal private receiver injection at actor entry points, using real actors with fake transports. Test initial dial, handshake, send backpressure, active-session wake and cancellation before implementing each change.

## Risks / Trade-offs

- Resume replaces even a surviving socket → idempotent existing cursor/outbox replay preserves updates; test pending delivery.
- Global event buses can interfere across async tests → use existing serialization or scoped receiver injection.
- Wake during bootstrap may lose readiness → retain the readiness sender until the next successful handshake, and test pre-ready interruption.

## Migration Plan

No data or protocol migration. Reverting the actor lifecycle changes restores previous timing while durable state stays compatible.
