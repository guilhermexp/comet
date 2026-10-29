# Tasks

## 1. Runtime discovery

- [x] 1.1 Record local package identity/layout and native initialization evidence without provider, network or audio operations in `.tmp/evidence/fix-codex-voice-runtime-resolution/`.
- [x] 1.2 Add bounded active-package discovery and package validation with offline resolver regression fixtures; update voice-media DOX and inspect the scoped diff.
- [x] 1.3 Route the local host through the shared resolver, add an isolated host-adapter fixture, and update harness DOX; verify both local and desktop paths use the shared resolution contract.

## 2. Integration checks

- [x] 2.1 Validate the change with `openspec validate fix-codex-voice-runtime-resolution --strict` and run `git diff --check`; record exact pending Cargo filters and limitations.
- [x] 2.2 Parent: run grouped Cargo formatting/tests and the required review-pair gate on the integrated snapshot; record results before final commit.

## Workflow follow-up

- Parent accepts the delivery and archives the change after verification; no archive or final commit from this worker.

## Integrated acceptance

- `zeron-voice-media`: 12 passed, 1 opt-in packaged-runtime test ignored; the real installed helper initialization was exercised separately with network denied. Codex realtime host: 2 passed. Engine voice runtime: 4 passed. Logs: `.tmp/verify/runtime-regressions-20261028/checks-1/` and `checks-4-green/`.
- No real microphone/provider conversation, CLI installation/update or change to active sessions was performed. The earlier disappearance of `standalone/current` remains unproven.
- Review: `.tmp/verify/runtime-regressions-20261028/review-pair/5/summary.md`, both reviewers completed, secrets clean, exit 6 for one finding.
- **Refutation — Windows entrypoint join:** the finding claims that `root.join("bin/codex.exe")` appends a slash-containing single component to a verbatim Windows path. Rust 1.98.0's standard library disproves that premise: `std/src/path.rs:3077-3084` implements `join` through `PathBuf::push`; `path.rs:1369-1407` handles a verbatim prefix by iterating the incoming relative path's components and rebuilding them with `MAIN_SEPARATOR_STR`. Thus `bin/codex.exe` and `codex-resources/voice` are decomposed and normalized, not appended as a single verbatim component. Official Windows tests at `std/tests/path.rs:1760-1767` cover forward slashes appended to verbatim roots. Sources inspected in `/opt/homebrew/Cellar/rust/1.98.0/lib/rustlib/src/rust/library/`. No Windows execution is claimed; the specific deterministic standard-library claim is false, so no production workaround was added.
- Later review round 6 found an empty `CODEX_HOME` suppressing the HOME fallback. A process-isolated public-resolver regression failed before the one-line empty-value filter and passed afterward. Red/green logs: `.tmp/verify/runtime-regressions-20261028/checks-4-{red,green}/`. Round 6 itself did not release the gate because CodeRabbit was quota-limited; a final complete pair remains required before commit.
- `.tmp/verify/runtime-regressions-20261028/review-pair/8/summary.md` repeats the Windows join finding. The source-based refutation above applies unchanged: the relative operand is decomposed before the verbatim result is rebuilt. This is not evidence of Windows runtime execution, nor a reason to add extra path copies.
- **Refutation — documented test filters (round 12):** `.tmp/verify/runtime-regressions-20261028/review-pair/12/summary.md` requests confirmation that `media_resolution` and `installed_package` are nonempty filters. They are already covered by actual execution: `.tmp/verify/runtime-regressions-20261028/checks-4-green/voice-media.log:199-207` contains six successful `tests::media_resolution_*` cases and one successful `tests::installed_package_rejects_wrong_identity_layout_target_and_version`. The documented commands therefore do not silently select zero tests; no documentation or extra name-list-only test run is needed.
