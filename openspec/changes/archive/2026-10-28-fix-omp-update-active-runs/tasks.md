# Tasks

## 1. Diagnose and preserve active OMP work

- [x] 1.1 Trace admission, retirement and supplier replacement; record cause and conservative installation tradeoff in design.md.
- [x] 1.2 Add active-input/background/accepted-continuation regressions, scope the fix to OMP and update engine DOX; artifacts inspected, execution remains task 2.1.
- [x] 1.3 Add a long subprocess/fictitious update/new execution regression through the real coordinator; fixture smoke passed, Rust execution remains task 2.1.

## 2. Integrated verification

- [x] 2.1 Parent runs grouped Cargo checks, formatting and the required review-pair gate on the integrated snapshot; retain the prior RPC chunk regression.
- [x] 2.2 Validate the change and deliver evidence, changed paths and unrun commands for parent acceptance (`openspec validate --strict`, `git diff --check`, `.tmp/evidence/fix-omp-update-active-runs/REPORT.md`).

## Workflow follow-up

- Parent accepts and archives; do not archive or create a final commit in this worktree.

## Integrated acceptance

- Passed: 45 updater tests, 18 runtime-survival tests, 37 message-queue tests, 4 engine voice-runtime tests and 60 OMP RPC tests (2 opt-in RPC cases ignored). The strengthened fictitious-update test passed separately and asserts the coordinator reaches Updated with version 2.0.0 and no failure.
- Independent subprocess smoke preserved old-process replies for turn/tool/steer/Worker notice, then observed 2.0.0 on the next execution. No installed OMP update or real provider was exercised. Logs: `.tmp/verify/runtime-regressions-20261028/checks-1/` and `checks-2/`.
- Both reviewers completed in `.tmp/verify/runtime-regressions-20261028/review-pair/5/summary.md`; secrets clean. Its sole remaining finding is refuted in the voice change's tasks. The earlier fixture version-argument mismatch was fixed, and the strengthened test passed afterward.
