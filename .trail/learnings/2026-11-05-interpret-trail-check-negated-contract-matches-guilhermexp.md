---
title: Interpret trail check negated-contract matches with source evidence
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
Trail check 0.23 uses lexical matches and can flag a change that enforces a learning as bringing back the rejected behavior. Validate each reported conflict against the exact implementation and behavioral evidence. Preserve the exit status and reason in the review receipt and commit message when the source proves the change is correct.

## Tried and ruled out
Treating every keyword match as a semantic conflict would reject the native-timing fix's tests, documentation and removal of the fallback itself. Do not rewrite approved planning documents or remove valuable regressions just to silence these matches.

## Watch out
The native-timing/context change produced negated-contract matches in docs, tests and mailbox code, including approved planning files outside its delivery diff. Manual source review and red/green behavioral tests resolved these as false positives; an exit 1 is still reported as exit 1, never as an automatic pass.
