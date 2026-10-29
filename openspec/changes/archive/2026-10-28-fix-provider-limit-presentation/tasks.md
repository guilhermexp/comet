# Tasks

## 1. Shared provider error presentation

- [x] 1.1 Implement pure classification/formatting with regression cases for Anthropic, OMP wrappers, timing and unknown errors; document proto contract. Verify via run_errors unit tests (execution deferred to parent).

## 2. Desktop consumers

- [x] 2.1 Use shared messages in transcript, composer failures and status strip; retain auth-only login. Add consumer and GPUI button tests, migrate the existing mobile sign-in consumer to the same authentication-only action contract, and update UI/mobile contracts. Verify via focused tests (execution deferred to parent).

## 3. Integration evidence

- [x] 3.1 Validate OpenSpec and diff hygiene; record observed cause, exact deferred commands and native smoke limits in .tmp/evidence/fix-provider-limit-presentation.
- [x] 3.2 Parent runs grouped Cargo checks, isolated native smoke if viable and integrated review-pair gate before acceptance.

## Workflow follow-up

- Parent accepts and archives the change after integrated validation. No commit, push or archive in this worker branch.

## Integrated acceptance

- Proto: 7 focused tests passed; UI: 46 focused tests passed, including the real authentication-only button; existing mobile card regression passed. Native production cards captured in `.tmp/evidence/fix-provider-limit-presentation/native-3.png`, without Shell/engine/providers.
- Final focused confirmation after source freeze: 7 proto + 3 UI tests passed (`.tmp/verify/runtime-regressions-20261028/checks-1/`, `checks-2/`, `checks-3/`; additional confirmation in the parent tool transcript).
- Both reviewers completed in `.tmp/verify/runtime-regressions-20261028/review-pair/5/summary.md`; secrets clean. Its sole finding concerns Windows path joining in the voice slice and is refuted with standard-library source in that change's tasks. Earlier valid findings were fixed and rerun.
