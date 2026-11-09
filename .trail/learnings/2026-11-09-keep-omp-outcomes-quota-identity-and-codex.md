---
title: "Keep OMP outcomes, quota identity and Settings availability authoritative"
author: Codex
date: 2026-11-09
branch: fix/blaze-omp-usage-context-20261109
touches:
  - crates/harness/src/omp/mod.rs
  - crates/engine/src/agent_accounts.rs
  - crates/engine/src/context_breakdown.rs
  - crates/workers-unpeel/src/controller_mcp.rs
---

## Decided

OMP prompt identity and per-request admission must exist before send; verified >=18.18.0 uses correlated prompt_result, while older/unknown binaries retain native terminal behavior. A negative admission ends immediately; an ambiguous failure after send may await its result. Framing v2 does not establish lifecycle capability. Preserve native message_end accounting and host tools before resume.

Codex quota account_id names the authenticated workspace, whereas the saved slot remains user::workspace. Validate both boundaries and reject superseded credentials. Cached quota receipt time belongs to the measurement, never a successful UI RPC. Runtime context fallback is Partial, bound to process/configuration/model/native session; enqueue alone cannot confirm reconfiguration, and absent occupancy is not zero.

workers list_presets must consume the existing Settings projection and enabled && installed, preserving screen order, separate favorites, custom commands and initialized empty state. Bootstrap enabled alone cannot establish local availability.

## Tried and ruled out

No new transport, polling, detector, OAuth refresher, queue restoration or cachewarming is needed. No global OMP replacement or paid provider prompt is justified by these consumer corrections. Cargo is the build/test proof: the retained Blaze pilot is blocked before compilation by missing vendored block/test_utils/Cargo.toml and proves no speedup.

## Watch out

Trail check's lexical conflicts must be inspected at exact spans: the native-timing prohibition, synthetic native-session fixtures, Steered confirmation and project-action coverage text do not restore the ruled-out behavior. Record each disposition rather than reporting exit 1 as success. Native screenshots, a compatible external OMP producer smoke, the user's real 401 recovery and an installed live preset tool are separate proofs; deterministic Cargo/HTTP/GPUI fixtures do not prove those external states. The orchestrator owns acceptance and archiving.
