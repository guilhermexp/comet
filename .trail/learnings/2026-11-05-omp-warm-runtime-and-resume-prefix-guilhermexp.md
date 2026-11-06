---
title: OMP warm runtime and resume prefix (prompt cache)
author: Guilhermexp
date: 2026-11-05
branch: fix/omp-prompt-cache-across-turns
cost: { minutes: 58, tokens: 480938 }
touches:
  - crates/engine/AGENTS.md
  - crates/engine/tests/omp_warm_runtime.rs
  - crates/harness/AGENTS.md
  - crates/harness/src/omp/mod.rs
  - crates/harness/tests/fixtures/fake-omp-rpc.sh
  - crates/harness/tests/omp_live_cache.rs
  - crates/harness/tests/omp_rpc.rs
  - openspec/changes/preserve-omp-prompt-cache-across-turns/tasks.md
  - crates/ui/examples/accounts-a11y-fixture.rs
  - Cargo.lock
---

## Decided
- OMP adapter keeps process/bridges alive after Done; idle message becomes `prompt` + `Steered`; engine parking unchanged (sessions.rs needed no edit).
- Resume order is `set_host_tools` BEFORE `switch_session`: omp builds the resumed system prompt/tool roster inside switch_session. Found by diffing PI_REQ_DEBUG=1 request bodies (rr-session-N.json in omp's cwd); fix leaves the prefix byte-identical.
- Live proof: `COMET_OMP_LIVE_CACHE=1 cargo test -p zeron-harness --test omp_live_cache -- --nocapture` (restart-per-turn and one-warm-process tests).

## Tried and ruled out
- Diffing without masking the billing-header `cch=` hash: it differs even in the native `omp -p -c` control and still hits cache.
- Reusing identical prompts across live runs: Anthropic's 1h cache makes reruns look like hits; the test uses a per-run nonce.
- Trivial echo prompts: Sonnet skips adaptive thinking, so `thinking_dropped == 0` was vacuous; the test now needs a thinking block.

## Watch out
- Tests that collect the OMP stream "to its end" now hang: read until Done.
- `cargo fmt --all` reformats the unrelated, already-unformatted `crates/ui/examples/accounts-a11y-fixture.rs`; revert it. Cargo also rewrites Cargo.lock and graft rewrites .cursor/hooks.json: restore both before committing.
- Never print captured request headers (authorization); scrub captures first.
