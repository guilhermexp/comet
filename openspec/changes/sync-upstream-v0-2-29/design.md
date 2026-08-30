## Context

See `proposal.md` for motivation and the capability specs for observable behavior. The current `main` is clean at `6519eb68`; its tree is independently based on upstream behavior through `v0.2.18`, but its Git ancestry diverges at `v0.1.17`. `upstream/main` is `b3fa5187`, and 60 of the 127 upstream-touched paths also contain private fork changes. A normal merge can therefore succeed syntactically while silently replacing or omitting product contracts.

## Goals / Non-Goals

**Goals:**

- Record upstream ancestry in an isolated review branch and preserve an auditable upstream marker.
- Reconcile overlapping code by behavior, with upstream regression tests ported before their implementation hunks wherever the fork does not already satisfy the behavior.
- Preserve all private capabilities and local-first behavior while taking compatible upstream fixes and features.
- Produce a clean branch with full Rust, edge, OpenSpec, packaging, and targeted native UI evidence.

**Non-Goals:**

- Promoting the integration branch to `main`, pushing it, creating a `v*` tag, deploying edge code, or uploading TestFlight builds.
- Replacing the fork's branding, Managed Provider Usage, updater decision, OpenSpec/DOX framework, Workers MCP/runtime, transcript export, or dev inspector with upstream defaults.
- Treating a green compiler as proof of native visual parity.

## Decisions

### D1. Integrate in a retained worktree and branch

Use `chore/upstream-sync-v0.2.29` in a sibling worktree, created from the clean local `main`. The worktree remains available after validation. This follows the repository's upstream-sync contract and makes rollback a branch deletion rather than a destructive reset.

Alternative considered: merge directly into `main`. Rejected because 60 overlapping paths and release/deploy workflows make an in-place merge unnecessarily risky.

### D2. Seed upstream provenance at the real behavioral baseline

Record upstream `v0.2.18` (`04b08ea2`) as the prior synchronized marker even though the fork's equivalent version commit has rewritten ancestry. The sync report uses that marker for the upstream release interval, while code review uses tree and behavior comparisons rather than ancestry counts alone.

Alternative considered: use the raw merge-base at `v0.1.17`. Rejected because it reports hundreds of already incorporated upstream commits as new and produces an unusable review surface.

### D3. Merge conservatively, then reconcile each overlapping capability

The initial merge records upstream ancestry with local priority. For every capability, import upstream tests or write equivalent fork-native tests first, observe RED when behavior is absent, then port the minimal implementation. If a test passes immediately because the fork already implements equivalent behavior, retain the fork implementation and document the equivalence instead of duplicating it.

Alternative considered: accept the upstream tree wholesale. Rejected because it would remove private files and regress local product decisions. A pure local overlay alone is also insufficient because it would hide fixes in the 60 overlapping paths.

### D4. Port in risk order

The order is registry/sync correctness, wire compatibility, OpenCode runtime, transcript/subagent semantics, then appearance/navigation. This lets durability gates settle before broad UI changes and keeps failures attributable to one subsystem.

Alternative considered: release-by-release cherry-picks. Rejected because each upstream release contains merge commits across shared UI and protocol files; capability-based ports create smaller observable contracts.

### D5. Preserve fork-controlled publication and identity

Keep the fork's package identity, current fork version policy, updater endpoints/behavior, deployment workflows, and private branding unless a capability spec explicitly requires otherwise. Upstream TestFlight and release-only commits may exist in ancestry but are not enabled or promoted.

Alternative considered: bump the fork to upstream `0.2.29`. Rejected because upstream version identity is not proof of fork release readiness and would conflate code intake with publication.

### D6. Treat UI validation as a separate evidence tier

Automated tests cover projection, state, geometry, and parsing. Native GPUI smoke validates theme import, typography, model picker, sidebar navigation, transcript disclosures, completion popups, and sticky Changes headers. Any unexecuted visual case remains explicitly human-needed.

## Risks / Trade-offs

- **Large semantic overlap** → Reconcile by capability and preserve local files by default; review every one of the 60 overlapping paths before completion.
- **Wire incompatibility across devices** → Port additive/lenient decoders first and run Rust plus edge compatibility suites before UI work.
- **OpenCode driver conflicts with Workers projection** → Keep tagged parent/child tests as a hard gate and do not remove the fork's event projection until native equivalents pass.
- **Theme dependencies or licenses conflict with the fork** → Admit only MIT-compatible code/assets, preserve notices, and keep GPL Zed crates excluded.
- **Merge history contains publication workflows** → Do not push the branch; keep deploy, release, TestFlight, updater, and tag operations outside validation.
- **Automated UI tests miss visual regressions** → Retain the worktree for native review and report visual UAT separately from green code gates.

## Migration Plan

1. Commit these planning artifacts on local `main`, format the existing fork, and verify the checkout is clean.
2. Create the isolated sync worktree/branch and record `04b08ea2` as the previous upstream marker.
3. Merge `upstream/main` with local-priority conflict handling and no push.
4. Reconcile capabilities in D4 order using RED→GREEN focal tests and small commits.
5. Update DOX, provenance, and the sync report; run `cargo test`, edge tests/typecheck, `cargo build -p zeron`, formatting, OpenSpec strict validation, and diff checks.
6. Run available native smoke checks and leave any remaining visual-only scenarios as human-needed.
7. Keep the clean worktree and branch for explicit review. Promotion and publication require a separate explicit approval.

Rollback is to leave `main` unchanged and discard the isolated branch/worktree with `git worktree remove` only after confirming it is clean.
