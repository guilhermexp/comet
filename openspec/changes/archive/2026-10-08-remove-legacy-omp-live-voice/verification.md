# Verification

## Contract and regression

- RED: the new RPC retirement regression failed before removal because `ProbeLiveVoice` was still registered (0 passed, 1 expected failure).
- GREEN: RPC library 23 passed; shared proto library 87 passed; relay local-only ingress regression 1 passed. The registry proves the five obsolete methods are absent while Codex voice methods remain.
- OMP integration suite: 61 passed, 2 pre-existing external CLI tests ignored; ordinary transport, session resume, steering and Worker notifications retained.
- Ordinary steering regressions: 6 passed; Codex voice runtime integration: 4 passed, including remote control, owner-drop cleanup and failed-prepare retry.
- Composer-focused UI tests: 286 passed; Codex idle startup: 2 passed; root voice-orchestrator tool grant: 1 passed.
- Retained Codex UI availability: 1 passed; voice stage/navigation/return affordance: 6 passed; draft-first-send and normal Chat-record creation: 1 passed each. These final UI checks compiled the cleaned-up imports.
- Engine retirement regression: 1 passed; shutdown regressions: 5 passed. Legacy methods return unknown-method errors without creating or mutating Chat state.
- The obsolete protocol unit filter matched 0 tests and is not counted as coverage; retained protocol negotiation/chunking cases are included in the 61 OMP integration tests.
- All 62 canonical OpenSpec specs validated strictly; the retirement delta also validated strictly.
- `cargo fmt --all` and `git diff --check` passed.

## Independent review

Reviewed harness transport and shared fixture, proto/RPC boundary, engine command/shutdown hooks, composer/state, lifecycle and sound cleanup. No material runtime findings remained. A moved-error borrow issue in the new engine test was corrected during review.

The first focused harness compile found `BoxStream` was also used by an ordinary test helper. Restored that import and removed the newly unused production `Path` import; an independent follow-up checked all remaining references, including platform configurations. Final UI compilation also exposed obsolete `CheckoutPlan` and `SessionStatus` top-level imports; removed those imports while retaining qualified and test-local uses. No test threshold or assertion was weakened to repair compilation.

## Native acceptance and closeout

- Rebuilt with normal `PATH="$HOME/.local/bin:$PATH" CARGO_BUILD_JOBS=4 cargo run` from the primary checkout. The changed dev build completed in 1m 25s; the final single-instance launch reused it and started its own updated engine on local IPC 27654 with the real `~/.zeron` profile.
- A stale second headed instance was closed before final acceptance, so the final UI did not attach to the previous engine. Engine logs confirm startup of the current engine and the installed standalone Codex 0.161.0 binary.
- Inspected an existing local OMP Chat: input contains attachments/send and no legacy OMP microphone or live strip; real account, Chat history and sidebar Codex microphone remain visible.
- Inspected a new local Chat draft: input also has no OMP microphone or live strip; project/device/model selection and sidebar Codex microphone remain visible. No message or provider voice call was submitted for this acceptance.
- Existing profile/account data remains in place. The active-call Codex return affordance is covered by the retained composer/stage unit checks; no live call was started by the automated verification.
- Independent review completed; owner DOX docs and canonical specs were synchronized. Each added/modified delta requirement matches the canonical spec; every removed requirement is absent there.
- Graft refreshed after final code cleanup. Strict OpenSpec validation and archive complete the retirement change; test/build logs remain in ignored `.tmp/retire-omp-voice/`.
