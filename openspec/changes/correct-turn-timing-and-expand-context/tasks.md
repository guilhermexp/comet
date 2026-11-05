# Tasks

## 1. Correct native Turn stats

- [ ] 1.1 Add focused failing regressions for fractional OMP durations, missing/invalid/zero timing and a partially measured multi-request turn; verify the failure reflects the current model-time substitution, not a wording assertion.
- [ ] 1.2 Preserve checked native duration and explicit incomplete contributions through all TurnMetrics producers/consumers, remove accumulator/UI wall-time fallbacks, and verify the regressions pass with unchanged usage/cache/cost and child exclusion.
- [ ] 1.3 Replay the recorded native turn through the real normalizer/accumulator, verify approximately 143.6s and 96.3 tok/s from 13,824 output tokens, and document the measured/unavailable behavior in the existing relevant user documentation/changelog.

## 2. Inspect native context safely

- [ ] 2.1 Add failing adapter contract coverage for correlated context inspection between turns and while active, version/unknown-command rejection, timeout/exit and continued normal prompting; prove the inspection causes no additional prompt, visible turn or provider request.
- [ ] 2.2 Implement the narrow TurnControl inspection mailbox, Claude summary-only/version-gated control handling and OMP schema-v1 RPC consumption; verify native category/list/provenance normalization and all cases from 2.1 pass without full-count fallback.
- [ ] 2.3 Add the typed host-forwardable GetChatContextBreakdown path and latest-only host cache; verify target-device/chat/runtime scoping, late-response rejection, changed-model invalidation and stale retained snapshots with focused host/RPC tests. Keep details out of synced documents.
- [ ] 2.4 Document optional inspection compatibility, estimate semantics and no-model behavior in the existing protocol documentation; verify examples against the typed adapter fixtures and coordinated OMP contract.

## 3. Expand Session Context inline

- [ ] 3.1 Add behavior regressions for isolated persisted expansion, zero/unknown/over-capacity values, deferred exclusion, nested non-additivity and stale/partial/unsupported states; verify failure against the current compact-only implementation.
- [ ] 3.2 Implement the accessible inline Context disclosure and budget/list projection using existing Session styling and per-context preferences; verify keyboard activation, independent MCP/memory disclosures, bounded lists and late-selection safety with the safe UI harness.
- [ ] 3.3 Review supported light/dark and narrow layouts under Impeccable and DESIGN.md without launching a dev Comet copy; record actual evidence and the installed-surface visual limitation. Update the existing user-facing documentation for the disclosure and availability states.

## 4. Prove coordinated integration and review

- [ ] 4.1 Consume output from the source-built OMP's real get_context_breakdown through Comet's adapter/host path using a controlled local provider; verify no extra provider call, stable prompt/tools/messages/usage and old-runtime unavailability. Preserve sanitized command/output receipts.
- [ ] 4.2 Exercise the installed Claude summary control without generating a model prompt, and run canonical focused Comet checks plus persistent-runtime/cache regressions. Route any required heavy Rust suite through box-test; report only checks actually exercised and isolate unrelated failures.
- [ ] 4.3 Run /Users/guilhermevarela/orchestrator.v5/scripts/review-pair.sh with --gate on the final diff before local delivery commits; resolve findings or complete the documented immediate manual contingency for quota/service unavailability, preserve secret scanning, and record the real review modality and receipt paths here.

## Destinations and evidence

- Local implementation/integration in isolated task branches, with scoped local delivery commits after plan approval. Preserve `refine-desktop-voice-surfaces` and all unrelated working-tree changes.
- OMP producer work is owned by the coordinated `expose-session-context-breakdown` change, not duplicated in this checklist. The shared report v1 is defined in that change's design.
- No release/feed update, push, installed Comet launch, fork publication or installed OMP replacement is authorized by this planning step. Do not mark installed behavior proven from unit tests.
- Functional, review and visual-limit receipts belong under `.tmp/verify/correct-turn-timing-and-expand-context/` and must be linked before acceptance.

## Workflow follow-up

- After accepted verification/review, archive the change with the CLI and verify the canonical spec result; synchronize only applicable AGENTS.md guidance.
- Perform mandatory local closure for every task checkout/build/service and box-test checkout actually used. Preserve receipts, unrelated work and any unintegrated OMP branch; state safe retentions and real disk-space evidence.
- Release and installed-surface acceptance remain separate destinations until authorized, rather than implicit success claims.
