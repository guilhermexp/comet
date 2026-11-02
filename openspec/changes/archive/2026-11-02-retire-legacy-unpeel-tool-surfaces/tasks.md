# Tasks

## 1. Comet policy (F1/F3)
- [x] 1.1 Add regressions for launch/resume grants, `tools/list` and refused direct calls under the Comet policy, and unchanged standalone defaults.
- [x] 1.2 Add `unpeel_core::embedding` policy; apply it in launch, resume and the MCP host; register it from Comet's vendored dispatch.
- [x] 1.3 Remove Comet's `__browser_cleanup__`/`__computer_cleanup__` routes and their test pins; keep vendored modules for standalone.

## 2. Alias cutover (F5)
- [x] 2.1 Replace `legacy_tool_names_still_dispatch` with a rejected-call/no-side-effect regression; keep canonical-action coverage.
- [x] 2.2 Remove `legacy_sessions_action`, the `browser_` prefix route and the familial names; rewrite help/error strings that teach removed names.
- [x] 2.3 Update vendored docs, `third_party/unpeel/AGENTS.md`, `crates/workers-unpeel/AGENTS.md` and `third_party/unpeel-upstream.toml` provenance.

## 3. Verification
- [x] 3.1 `cargo test --manifest-path third_party/unpeel/crates/Cargo.toml -p unpeel-core --lib` and `cargo test -p zeron-workers-unpeel` focused suites; `openspec validate --strict`.
- [x] 3.2 Mac acceptance on the common Comet boundary, not per provider: the current-source default `zeron` binary ran `__mcp__` over real stdio in isolated temporary profiles; `tools/list` contained only `sessions`; retired domains and removed aliases returned errors without delivery or session artifacts, and EOF exited 0. `comet-computer-use` remains separately registered/configured, not advertised by `unpeel`. The GR23 smoke also verified four retired browser aliases point to the integrated Browser panel, never the retired tool. Mac evidence: `.tmp/verify/desktop-corrections/mac/operational-final/{acceptance.json,mcp/f1f3f5-f7-result.json,hint/gr23-result.json}` (20 protocol checks and 15 hint checks, independently checked IDs/JSON/schema). No real provider/private device/audio or user profile was accessed; even the supervisor HOME was temporary.

Final Mac package review: `.tmp/verify/desktop-corrections/final-review-receipt.json` records the actual review mode, reviewed tree and automatic/manual summary path.
