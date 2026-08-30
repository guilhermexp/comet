# Runtime reliability cleanup design

## Scope

This cleanup addresses three independent failure classes observed during a successful native run: Chat sync quota amplification, native runtime integrity errors, and macOS build/runtime diagnostic noise. The fork stays on its current app version and pinned upstream revisions; updater behavior and the reported newer upstream release are explicitly out of scope.

## Decisions

- **D1 — Backpressure:** quota rejection blocks eager sends for that Chat connection until the existing retry deadline. Local edits remain queued; the retry sends one head item, and acknowledgements restore ordered draining.
- **D2 — Runtime integrity:** diagnose the precise GPUI callback and malformed transcript field before changing behavior. Fix the producer/callback at its source; keep GPUI borrow checks and transcript salvage intact.
- **D3 — Diagnostics:** use narrow compatibility measures: one declared legacy cfg value, virtual routing to existing embedded fonts, and minimal transitive dependency patches only where fixed compatible releases cannot be selected.
- **D4 — Boundaries:** do not change updater logic, version `0.2.18`, release automation, edge quotas, protocol formats, or the pinned GPUI upstream revision.

## Verification strategy

Every implementation starts with a failing regression test or reproducible diagnostic gate. Focused crate tests run first, followed by `cargo fmt --all`, `cargo test`, `cargo build -p zeron`, and a bounded native macOS smoke run whose output is checked for the targeted messages. Native GPUI proof must exercise the real interaction path; unit tests alone do not prove the absence of window reentrancy.
