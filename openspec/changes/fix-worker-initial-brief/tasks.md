## 1. Complete runtime coverage

- [x] 1.1 Add formatted behavioral cases for all four runtimes to worker_initial_briefing; capture a behavioral RED with unchanged test bytes and exact failing source preserved.
- [x] 1.2 Generalize native delivery through the existing integration descriptors; migrate every OMP-specific module reference, preserve reservation/ACK and parent semantics, and remove raw Codex argv trace logging through the normal hook update path.
- [x] 1.3 Prove literal payloads, leading options, trailing newlines, original preset flags, missing-body/spawn/ACK failures, diagnostic privacy and restart without replay for each transport/runtime.

## 2. Validation and delivery

- [x] 2.1 Update affected DOX and vendor provenance; run formatter before final gate, typecheck, focused gate and the canonical workspace suite, then commit and capture the production build receipt.
- [x] 2.2 Main reviews source/security, integrates without touching existing WIP and updates the installed Comet atomically.
- [x] 2.3 Main observes OMP executing its initial task through installed launch_worker without a later task send.
- [x] 2.4 Main observes Claude executing its initial task through installed launch_worker without a later task send.
- [x] 2.5 Main observes Pi executing its initial task through installed launch_worker without a later task send.
- [x] 2.6 Main observes Codex executing its initial task through installed launch_worker without a later task send.
- [x] 2.7 Record honest permission/authentication interactions and limits, verify no replay, clean up only probe processes/data, and reconcile this checklist from observed results.

Final fingerprint-bound receipts and independent acceptance are recorded in the work ticket after this source checklist is frozen. Keeping that final state in the ticket avoids invalidating its own evidence by changing a source checkbox.
