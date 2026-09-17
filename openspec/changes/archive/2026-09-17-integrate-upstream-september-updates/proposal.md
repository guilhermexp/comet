## Why
Bring the missing upstream September improvements into the fork without losing its existing runtime and UI contracts. The user approved integration on a new branch following the comparison against upstream main 8ee7a622.

## What Changes
- Correct empty Enter interruption, durable sync publication, preview probing, Codex login and code selection.
- Add command palette, independent terminal/code fonts, generated image display and Appshots.
- Reconcile necessary menu and OpenCode correctness fixes with the fork; preserve the current visual design. Broad Windows support and composer redesign remain outside this nine-feature integration.

## Capabilities
### New Capabilities
- `september-upstream-integration`: missing upstream user behavior with fork compatibility.
### Modified Capabilities
None. Existing OMP, Workers, Live Voice, theme, steering and remote Files contracts remain binding.

## Impact
Rust UI, engine, sync, preview, harness, wire additions where needed, platform support and iOS presentation. Work stays on integrate/upstream-2026-09-16. No upstream push or deployment.
