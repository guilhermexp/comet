## Implementation
- [x] GJ1 publish stable observed version and extend cancellation/cache regression
- [x] GJ2 pair executable/version/release and retain same-family behavior
- [x] GJ3 creator query and separate lineage summaries
- [x] GJ4 failed native initialization reports without panic
- [x] GJ5 narrow typed candidate discovery errors
- [x] GJ6 painted Details width under opening resize
- [x] GJ7 existing literal-key help/schema metadata
- [x] GJ10 stage mute readiness and same-controller proof

## Focused verification and handoff
- [x] Focused updater and MCP regressions observed
- [x] Safe Mac stage/native positive and negative acceptance observed — glass and opaque PASS (24 checks / 21 captures each, tree 09d7b70); final light PASS (24 checks / 21 captures, tree 81d5787), with unchanged shipping UI. Final hidden/init negatives each retained 21 captures and failed only their expected checks. Parent inspected real captures and verified all 7217 source hashes unchanged during the final run. Receipt: `.tmp/verify/desktop-corrections/mac/final-integration-closeout/acceptance-and-cleanup.json`. The original light window loss remains unproven; its failure and the controlled non-panic reporting proof are preserved. This acceptance is component-only, not a full app/provider/audio session.
- [x] Actual X11 protocol checks or exact platform blocker recorded
- [x] Shipping compile and fixture isolation verified
- [x] Self-review, owning DOX, source identity and evidence delivered
- [x] Parent acceptance and independent UI/native + lifecycle reviews completed without open findings — `.tmp/verify/desktop-corrections/local-codex-review-2/{native-review,lifecycle-review}.json`. Archiving records accepted implementation, not gate approval: the final commit still requires the final-snapshot gate at `.tmp/verify/desktop-corrections/review-pair/mac-final-4/summary.md`, or a complete recorded manual contingency if that gate is unavailable. No main merge, push, release or install is authorized.

## GJ12 follow-up
- [x] Atomic family invalidation before any captured-target conclusion; one guarded recheck after cleanup
- [x] Causal cancellation, future cleanup, current-version and opt-out regressions; focused engine compatibility checks
- [x] Owning DOX, source identity, self-review and delivery 2; preserve delivery 1
- [x] Fixture window-loss failure writes partial result.json and exits 1 without panic; controlled macOS close proof preserved its first PNG (visual rerun belongs to parent)
