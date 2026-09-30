# Design

## Context

See proposal.md. Comet includes the vendored TUI activity engine directly. The Comet bridge, TUI and native Swift client all write the same hook-port registry. Polling is the recovery path when broadcasts are lost.

## Goals / Non-Goals

Recover persisted lifecycle transitions and preserve listeners. Runtime commands, wire formats, generation guards and the safety timeout remain unchanged.

## Decisions

- Check durable event timestamps even after the session has latched hooks. Track the last consumed disk version separately from the latest applied live event; do not replay an unchanged seed or an older event. This preserves Codex output rearm and avoids repeated output-anchored Start revival.
- Remove age-based registry truncation in all three writers. Registration age cannot prove death; only explicit unregister removes an entry. Automatic stale-port collection needs a separate liveness contract and is outside this fix.
- Implement directly on main as requested. Tests and the final diff receive independent review.

## Risks / Trade-offs

- Stale registrations may remain after a crashed client, as they already do today. Hook posts retain bounded per-port transport deadlines; no disk format changes.
- Same-time file rewrites depend on filesystem timestamp precision, consistent with the existing lifecycle ordering model. Atomic hook writes are retained.
- Existing app processes keep the old code until reloaded; never terminate unrelated Worker hosts to refresh the app.
- Legacy hooks carry runtime generation but no source event sequence. A durable event older than the latest HTTP receipt stays rejected: otherwise a legitimate Start whose disk write failed would be rolled back to an old Stop. Arbitrarily reordered HTTP (including delayed Start after Stop) remains a pre-existing transport limitation; distinguishing it safely requires source identity in all provider reporters. This change repairs registry eviction and loss recovery for newer persisted events, including the reported incident; it does not claim arbitrary transport ordering.
