## Context
Authoritative design: .tmp/verify/desktop-corrections/cloud-round15-integration-edges.md and review-pair/mac-final-3/disposition.json, adapted by .tmp/briefs/desktop-corrections-local-codex-final.md.

## Goals / Non-Goals
Implement the eight scoped integration corrections. No GJ9 repeat/queue changes, reparenting, new authority, key restrictions, new installers or provider lifecycle. GJ8 duplicates GJ6; GJ11 duplicates GJ3.

## Decisions
1. Publish the successful stable older probe version with its fingerprint, without a second diagnostic.
2. Move the existing exclusive version observation before planning. Use that installation/version pair and compare the existing package/release identity; release admission and pending intent before scheduling the existing full check on mismatch. Preserve OMP same-family replacement.
3. Creator query matches either lineage; summaries expose both separately.
4. Fixture native hit probes return explicit unavailability (`None`, with the page initialization error in the report); callers cannot interpret absence as clearance. Construction fault injection is fixture-feature-only. Hidden real hosts keep real hit probes.
5. Skip only BadWindow for candidate attributes and BadDrawable for candidate geometry; preserve all other and selected-capture errors.
6. Details occupancy is zero for closed target, otherwise the same evaluated tween as actual layout. Test painted component/native bounds through resized positive-target intermediate frames.
7. Change peer schema/help only, using shared byte/count constants.
8. Stage receives can_mute; unavailable controls use connecting semantics and no actionable click; connected action reaches the same controller once.

## Verification
One canonical heavy-run/cargo-verify guarded run, jobs six inside private driver, isolated component fixtures, causal updater/MCP regressions, actual help smoke, native positive/hidden/absent-host reports. Preserve historical evidence. No false red/green claim; platform blockers are explicit. Keep this change active until parent acceptance.

## GJ12 — candidate identity across observation and cancellation
Parent-approved follow-up: compare each stable observation against the original candidate's package family before interpreting the captured target, including already-current outcomes. Publish changed installed version and candidate invalidation atomically. The existing UpdateIntentGuard remembers one required full check and dispatches it only after pending intent is cleared, including cancellation and future cleanup, respecting shutdown, disabled harnesses and Off policy. No new installer, retry framework, production test hook or wire field. Same-family OMP remains unchanged.

Verification: disposable executable regressions for cross-family observe/cancel/reapply, future cleanup, exclusive-publication cancellation, and a numerically current cross-family replacement; retain same-family and install/cancel exclusion checks. Evidence: .tmp/verify/desktop-corrections/local-codex-final-2/. Parent owns native acceptance and final gate.

## Fixture window-loss reporting — parent QA follow-up
Parent's light run lost its window after 18 captures and aborted at window.update.unwrap; the cause of the window loss is unproven. Keep Report outside an anyhow::Result async exercise; propagate update/resize/presentation errors with phase context into a failed check and the existing serialization/exit-1 finalizer. Preserve prior capture receipts. No fallback probes, retries, reopen, close veto or shipping change. An example-only controlled close proves the missing-window path on macOS; it does not establish cross-platform autoquit guarantees.
