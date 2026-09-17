## Scoped runtime plan

This area ports the upstream Codex account resolver (`d8ce7b6e`), OpenCode
1.18/2.x protocol compatibility (`67c960f4`), and the Codex subagent identity,
spawn, lifecycle, and resume fixes (`8c90f236`, `a0cd3768`, `83b12432`, and
`b6fed691`). The fork's OMP background commands, steering, Live Voice, and
shell-resolution contracts remain authoritative.

1. Compare each upstream patch with the current harness and engine code,
   retaining behavior already present in the fork and adapting conflicts at
   the existing owners.
2. Port Codex login executable resolution and its regression coverage without
   widening the engine account boundary. Add the engine DOX contract through
   the parent owner rather than editing that shared document here.
3. Port OpenCode session creation, 1.18 compatibility, and 2.x wire parsing
   with fixtures/tests that exercise the supported protocol variants.
4. Port Codex subagent identity, v1 spawn events, v2 lifecycle distinction,
   and persistent routing/resume coverage without changing doc/proto surfaces
   owned by other areas.
5. Run focused harness and account tests, then record exact results and any
   intentionally deferred cross-crate pieces below.

The runtime also seeds generated-image replay from the durable parent and
subagent docs before a resumed run. The walk follows only deterministic
engine-generated spawn references, reads cold children from local snapshots
and pending outbox updates without joining historical rooms, and is bounded by
256 documents, 32 levels, and visited-doc cycle guards.

## Validation log

- `cargo test -p zeron-harness --lib` — **173 passed, 0 failed**.
- `cargo test -p zeron-harness --test codex` — **21 passed, 2 ignored, 0 failed** (including v1/v2/resume subagent fixtures).
- `cargo test -p zeron-harness --test opencode` — **14 passed, 0 failed**.
- `cargo test -p zeron-engine --test codex_login_resolver -- --test-threads=1` — **1 passed, 0 failed**.
- `cargo test -p zeron-engine sessions::tests::persisted_nested_image_replay_skips_removed_source_after_restart --lib` — **1 passed, 0 failed**.
- `cargo test -p zeron-engine generated_image --lib` — **4 passed, 0 failed**.

The Codex fixture suite keeps the fork's existing OMP/Workers/Live Voice and
shell-resolution behavior intact. Generated-image handling is limited to the
harness normalization and consumes only `savedPath`; profile upload/materialized
doc behavior remains owned by the parent engine/doc changes.
