# Tasks

## 1. Correct native Turn stats

- [x] 1.1 Add focused failing regressions for fractional OMP durations, missing/invalid/zero timing and a partially measured multi-request turn; verify the failure reflects the current model-time substitution, not a wording assertion.
- [x] 1.2 Preserve checked native duration and explicit incomplete contributions through all TurnMetrics producers/consumers, remove accumulator/UI wall-time fallbacks, and verify the regressions pass with unchanged usage/cache/cost and child exclusion.
- [x] 1.3 Replay the recorded native turn through the real normalizer/accumulator, verify approximately 143.6s and 96.3 tok/s from 13,824 output tokens, and document the measured/unavailable behavior in the existing relevant user documentation/changelog.

## 2. Inspect native context safely

- [x] 2.1 Add failing adapter contract coverage for correlated context inspection between turns and while active, version/unknown-command rejection, timeout/exit and continued normal prompting; prove the inspection causes no additional prompt, visible turn or provider request.
- [x] 2.2 Implement the narrow TurnControl inspection mailbox, Claude summary-only/version-gated control handling and OMP schema-v1 RPC consumption; verify native category/list/provenance normalization and all cases from 2.1 pass without full-count fallback.
- [x] 2.3 Add the typed host-forwardable GetChatContextBreakdown path and latest-only host cache; verify target-device/chat/runtime scoping, late-response rejection, changed-model invalidation and stale retained snapshots with focused host/RPC tests. Keep details out of synced documents.
- [x] 2.4 Document optional inspection compatibility, estimate semantics and no-model behavior in the existing protocol documentation; verify examples against the typed adapter fixtures and coordinated OMP contract.

## 3. Expand Session Context inline

- [x] 3.1 Add behavior regressions for isolated persisted expansion, zero/unknown/over-capacity values, deferred exclusion, nested non-additivity and stale/partial/unsupported states; verify failure against the current compact-only implementation.
- [x] 3.2 Implement the accessible inline Context disclosure and budget/list projection using existing Session styling and per-context preferences; verify keyboard activation, independent MCP/memory disclosures, bounded lists and late-selection safety with the safe UI harness.
- [x] 3.3 Review supported light/dark and narrow layouts under Impeccable and DESIGN.md without launching a dev Comet copy; record actual evidence and the installed-surface visual limitation. Update the existing user-facing documentation for the disclosure and availability states.

## 4. Prove coordinated integration and review

- [x] 4.1 Consume output from the source-built OMP's real get_context_breakdown through Comet's adapter/host path using a controlled local provider; verify no extra provider call, stable prompt/tools/messages/usage and old-runtime unavailability. Preserve sanitized command/output receipts.
- [x] 4.2 Exercise the installed Claude summary control without generating a model prompt, and run canonical focused Comet checks plus persistent-runtime/cache regressions. Route any required heavy Rust suite through box-test; report only checks actually exercised and isolate unrelated failures.
- [x] 4.3 Run /Users/guilhermevarela/orchestrator.v5/scripts/review-pair.sh with --gate on the final diff before local delivery commits; resolve findings or complete the documented immediate manual contingency for quota/service unavailability, preserve secret scanning, and record the real review modality and receipt paths here.

## Destinations and evidence

- Local implementation/integration in isolated task branches, with scoped local delivery commits after plan approval. Preserve `refine-desktop-voice-surfaces` and all unrelated working-tree changes.
- OMP producer work is owned by the coordinated `expose-session-context-breakdown` change, not duplicated in this checklist. The shared report v1 is defined in that change's design.
- No release/feed update, push, installed Comet launch, fork publication or installed OMP replacement is authorized by this planning step. Do not mark installed behavior proven from unit tests.
- Functional, review and visual-limit receipts belong under `.tmp/verify/correct-turn-timing-and-expand-context/` and must be linked before acceptance.

## Workflow follow-up

- After accepted verification/review, archive the change with the CLI and verify the canonical spec result; synchronize only applicable AGENTS.md guidance.
- Perform mandatory local closure for every task checkout/build/service and box-test checkout actually used. Preserve receipts, unrelated work and any unintegrated OMP branch; state safe retentions and real disk-space evidence.
- Release and installed-surface acceptance remain separate destinations until authorized, rather than implicit success claims.

## Worker evidence (local only)

- Consumer base `8b702ed645386da8d09b48bb53477d82a2f33563`; coordinated OMP producer `04b043e2e3b4ea6b4d797cd9711f8382c54c22ce`. Principal/main and independent ef5ddeee sidebar work untouched. No push/release/install, global runtime upgrade, dev app/demo launch or archive.
- Initial required `graft build` succeeded in this checkout; final graph refresh only here. Initial `trail check` exited1 with15 lexical negated-contract matches; final `trail-final-check.log` exited1 with20 matches, including approved planning files. Reviewed against source and behavioral evidence: the code removes wall fallback, isolates summary traffic and rejects uncertain config. A new learning records the false positives; existing learnings preserved.
- All receipts below are under `.tmp/verify/correct-turn-timing-and-expand-context/`.

### Native timing

- RED `timing-red.log` discarded fractional native timing; `accumulator-red.log` substituted 6000ms for missing timing. GREEN `harness-timing-green.log` (19 tests), `accumulator-green.log` and `accumulator-boundaries-green.log` (complete/missing/zero/partial/overflow/reset/child exclusion).
- Actual normalizer/accumulator private read-only replay passed in `native-replay-green.log`:18 requests, model143609ms (per-request rounding), output13824, rate96.2614tok/s, fresh38/read5017335/write52343/cost1.698843. No private conversation fixture or output.

### Context contracts and integration

- Core adapter RED `context-adapter-red-binary.log` (initial wrong fixture scenario was corrected separately); normalization RED `context-normalizer-red.log`; budget RED `context-budget-red.log`. Canonical focused commands/exits are enumerated in `focused-results.json` (12 invocations passed).
- Final coordinated contract RED `omp-producer-contract-red.log` (missing2 native items), GREEN `omp-producer-contract-green.log` (4 tests). Preserves strict integer1, full identity/lifecycle, hasLocalChanges nullable, signed BPE residual, nested sources, section availability, model schema policy, xdev discovery, historical provider/transcript indices and activeEstimateReason. Populated items are contract-test evidence; the real loopback smoke has measured empty MCP/memory lists.
- Host/RPC RED `host-rpc-red-corrected.log` (UnknownMethod), `deleted-chat-red.log` (late deleted Chat), `host-invalid-retention-red.log` (invalid native payload erased data). GREEN `host-retention-final.log` (3 tests), forwarding `host-forward-green.log` (actual two-engine relay). Runtime settings/owner/config/native binding are checked around cache and native reads. No runtime spawned for inspection; stale retention preserves original timestamps.
- Claude held-settle RED `claude-turn-settle-red.log`, GREEN `claude-adapter-final.log` (5 tests). Correlated summary replies do not reset turn settlement or change steering/config certainty. Unknown/older versions issue no context control, and pending/refused model changes remain unavailable. `omp-adapter-green.log` (4 tests) covers unknown-command cache, idle/active reads, timeout/exit and ordinary prompting. `omp-warm-engine-green.log` covers five persistent-runtime regressions.
- Installed Claude2.1.318 no-prompt summary success: `claude-summary-smoke.json`. Second isolated memory/MCP attempt still yielded empty lists (`claude-native-lists-smoke.json`). Read-only executable schema metadata: `claude-static-schema.json`; summary redacts memory files, [] is unavailable attribution, isLoaded is distinct from deferred. No populated-list live claim.
- Source-built real OMP → Comet proof: canonical opt-in `cargo test --locked -p zeron-engine --test context_inspection source_built_omp_context_inspection -- --ignored --nocapture` with COMET_OMP_CONTEXT_LAUNCH/ORIGIN_FILE/SMOKE_RECEIPT set to the coordinated isolated launcher and this receipt directory. `source-built-omp-host-smoke.log` passed; `source-built-omp-host-smoke.json` records metadata/counts/hashes only. Existing native idle process queried twice through actual adapter transport/normalizer:0 requests/0 messages. Shutdown awaited before next launcher (fixed runtime dir is serial). Full Engine/serializedRPC then proves active inspections1, completed1, next normal prompt2, two normal Done events, stable full identity/categories/provider hashes and no inspection journal/doc mutation. Initial Engine idle-start remains Codex-only: host before runtime returns unavailability without spawning; this initial-native-idle distinction is explicit in the receipt.
- Old installed OMP isolated attempts exited1 before reply (`old-omp-smoke-retry.json`), not live Unknown-command proof. The owner-supplied native finding plus deterministic adapter test establish update-required behavior. No installed runtime replaced.

### UI and checks

- Compact-only RED `ui-red-corrected.log`; unsupported-retention RED `ui-unsupported-retention-red.log`; final safe GPUI `ui-context-final.log` and coordinated `ui-coordinated-native-final.log` each30 passed, `ui-session-card-final.log`8 passed. Narrow/light/dark300/500px, pointer+full Enter/Space presses, isolated preference roundtrip, independent seven-row scroll viewports, zero/unknown/over-capacity and generation/stale safety. Native attribution branch confirmation is in `ui-native-attribution-final.log`.
- Actual installed appearance remains `notProven` under provider isolation policy. An idle-only runtime retirement without a watched Session boundary is observed on next Refresh; snapshot/model/time labeling avoids a current-runtime claim.
- Client scoped-unavailability RED `client-facade-red.log` (missing identity); final facade evidence `client-facade-green.log`.
- Broad backend box run `box-backend.log`: client suites passed, engine812 passed/1 failed/8 ignored, stopped in unchanged `doc_host::sync_lifecycle_tests::legacy_admission_rechecks_owner_and_retires_only_the_captured_wake` (Join vs Seed). Same exact focused macOS test failed (`sync-admission-focused-macos.log`). No claim of a baseline proof or broad green.
- Separate protocol/harness box run `box-final-protocol-harness.log` progressed through harness units/fixtures, then stopped in two unchanged Codex voice helper expectations (`external_voice_keeps_canonical_events_without_any_local_helper`, `idle_voice_runtime_has_no_initial_turn_and_preserves_mcp`: Unsupported vs NativeRuntimeUnavailable;49 passed/2 failed/4 ignored). No voice-surface changes made. Separate final proto/RPC box result: `box-final-proto-rpc.log`.
- OpenSpec strict validate passed (`openspec-validate.log`); scoped rustfmt and diff whitespace checks passed. Known unrelated accounts example formatting was preserved without repeated checks.

### Review and handoff

- Manual read-only backend and Impeccable reviewers examined affected contracts, tests and UI. Findings resolved: Claude turn-settle mutation, config uncertainty, cached late selection, invalid payload retention, expanded percent mismatch, unsupported retention, lifecycle observation wording and Unix-only smoke compilation (`cfg(unix)`). No remaining concrete findings in those follow-ups.
- Required review-pair/manual contingency and final file/tree identities are recorded in `manual-review/1/summary.md`; known quota receipt copied there. No CodeRabbit blind retry, --agent or --use-credits; no automatic no-mistakes. Secret scanning remains required and recorded.
- Orchestrator owns acceptance/integration/archive. Task sources/builds remain available for coordinated smoke: this checkout `.tmp/target-correct-turn-context`, ignored receipts/helpers, local graph; box area `feat-native-turn-stats-context-details-f50e5502`; producer checkout/runtime unchanged. No destructive cleanup before integration. Disk evidence and final delivery hash live in the delivery receipt.

### Final review follow-up

- Required `REVIEW_PAIR_CODERABBIT='' /Users/guilhermevarela/orchestrator.v5/scripts/review-pair.sh --repo <this-checkout> --base 8b702ed6 --out <this-checkout>/.tmp/verify/correct-turn-timing-and-expand-context/review-pair/1 --include-untracked --gate` completed exit4. OpenQodex completed with4 findings; CodeRabbit deliberately disabled for known quota, reported binary_missing with0 attempts; secret gate clean. This is not an automatic pair pass.
- Three scoped findings fixed with actual RED/GREEN: queued failed-refresh coalescing (`failed-refresh-coalescing-{red,green}.log`), bounded retention (`context-cache-bound-{red,green}.log`,32 entries/protected in-flight gates/pruned deleted or foreign Chats), and late settled usage (`late-settled-usage-{red,green}.log`,Idle/Errored only, no active counter refresh). Preexisting `.cursor/hooks.json` finding is outside the scoped delivery and unchanged by this worker.
- Missing native model honesty also has RED/GREEN (`native-model-absence-{red,green}.log`); configured model no longer fills missing observation evidence. Final manual backend/UI follow-ups checked these deltas and exact source hashes. `review-fix-results.json` records final focused exits; source-built integration was reconfirmed after host-cache fixes (`source-built-omp-host-final.log`).
- Final native appearance remains notProven. Broad unrelated sync/Codex helper failures remain explicit above; no baseline claim or full-suite pass. No waiting producer dependency remains. Acceptance/integration/archive and resolution of unrelated failures belong to the orchestrator.

## Aceitação local pelo orquestrador

- Integração sem conflito de código com a pílula azul de workers (`ef5ddeee`), preservando o hook local não staged. Na árvore combinada: `cargo test --locked -p zeron-ui --lib` aprovou 2.685 testes; o smoke real Engine/RPC → OMP source-built aprovou inspeções em idle/ativo/finalizado, sem requests ou eventos extras. Recibo `orchestrator-integrated-smoke.json`.
- `openspec archive correct-turn-timing-and-expand-context --yes` sincronizou seis requirements novas em `details-sidebar`, sem modificar/remover as nove anteriores; a spec canônica passou na validação estrita. Os contratos duráveis de Context e contadores independentes permanecem em `crates/ui/AGENTS.md`.
- Revisão manual da integração e identidade da árvore: `.tmp/verify/correct-turn-timing-and-expand-context/integration/manual-review/1/summary.md`; scanner staged separado em `secret-scan.json` na mesma pasta. Modalidade manual, não aprovação automática do par.
- Provas preservadas também no checkout principal. Aparência no Comet instalado continua `notProven`; sem push, release, instalação ou substituição global do runtime nesta entrega.
