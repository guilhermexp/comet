---
title: Native turn timing and read-only context inspection
author: Guilhermexp
date: 2026-11-05
branch: feat/native-turn-stats-context-details
touches:
  - crates/AGENTS.md
  - crates/client/src/client.rs
  - crates/engine/AGENTS.md
  - crates/engine/src/rpc.rs
  - crates/engine/src/sessions.rs
  - crates/engine/tests/device_routing.rs
  - crates/harness/AGENTS.md
  - crates/harness/src/claude/mod.rs
  - crates/harness/src/claude/normalize.rs
  - crates/harness/src/claude/wire.rs
---

## Decided
Use complete native model durations independently of tool and wall time. OMP assistant durations are floating-point milliseconds: finite nonnegative checked rounding preserves zero; an explicit incomplete contribution invalidates a partial sum. Keep token/cache/cost totals and child exclusion independent.

Context inspection has a bounded request/reply mailbox separate from prompts, steering and configuration. Inspect only an existing selected host/runtime; coalesce readers and validate Chat ownership/config/runtime before cached returns and after native completion. OMP resume file paths and native session UUIDs are different identities.

## Tried and ruled out
Wall time minus tools is not native model time. Claude summary replies must bypass the turn-settlement clock, including foreign/late request IDs. A native configuration request being enqueued is not proof the model switch succeeded; reject inspection while queued/pending/refused configuration is uncertain.

## Watch out
Claude get_context_usage must explicitly use detail summary and a verified version gate. Installed 2.1.318 summary redacts memory-file metadata; [] is not measured zero memory, and isLoaded is independent from deferred budgeting. OMP optional detail keys must follow the coordinated producer contract, never an invented parallel schema. Safe GPUI layout/interaction tests do not prove installed appearance or authorize a dev app launch.
