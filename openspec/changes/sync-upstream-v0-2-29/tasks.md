## 1. Isolated Integration Baseline

- [x] 1.1 Commit the validated OpenSpec/implementation plan, run `cargo fmt --all`, and confirm local `main` is clean at the recorded baseline.
- [x] 1.2 Create retained worktree branch `chore/upstream-sync-v0.2.29`, seed `fork_changelog.md` with upstream baseline `04b08ea2`, and merge `upstream/main` at `b3fa5187` without push.
- [ ] 1.3 Audit the merge report, all conflicts, new dependencies/licenses, publication workflows, and the 60 overlapping private paths; restore fork-controlled identity, updater, deployment, and release behavior.

## 2. Registry, Sync, and Wire Safety

- [x] 2.1 Port registry cursor contiguity tests from `1fc68435`, observe RED, implement the compatible doc/sync behavior, and run focused doc/sync tests.
- [x] 2.2 Port server-truth orphan sweep and unreadable-ack retry tests from `28eb39b6`, observe RED, implement engine/sync behavior, and run focused integration tests.
- [x] 2.3 Port forward-compatible unknown-harness Chat row tests from `5306be20`, observe RED, implement lenient optional config decoding, and run doc/proto/engine tests.
- [x] 2.4 Port bounded diff reconciliation tests from `74f4abef` and `2119bf0c`, observe RED where absent, implement the compatible engine behavior, and run diff-sync tests.

## 3. OpenCode and Subagent Runtime

- [ ] 3.1 Port native OpenCode HTTP/SSE startup, timeout, prompt-gating, resume, steering, and error tests from `bf634445` through `a401432f`; observe RED before replacing the ACP launch path.
- [ ] 3.2 Implement the native OpenCode runtime while preserving the fork's Workers MCP and tagged parent/child event contracts; run the complete OpenCode/ACP harness suites.
- [ ] 3.3 Port connected-provider model filtering from `4fd35579` and subagent binding fixes from `5019dc19`, `18987da3`, and `569793ed`; run focused model and subagent lifecycle tests.

## 4. Transcript and Changes Navigation

- [ ] 4.1 Characterize fork-equivalent first-class Reasoning before reconciling `aa9f8bf1`; retain the fork implementation when behavior is already covered.
- [ ] 4.2 Port transcript copy, viewport restoration, live anchoring, and Thinking-in-tool-groups behavior from `v0.2.19..v0.2.28` with RED→GREEN projection tests.
- [ ] 4.3 Port sticky Changes headers through `b3fa5187` with deterministic header-boundary tests, preserving the fork's Changes tabs, comments, and right-pane ownership.
- [ ] 4.4 Run sticky-turn-header and turn-step-tool-group regression suites plus headed transcript/Changes smoke.

## 5. Appearance, Picker, Sidebar, and Shortcuts

- [ ] 5.1 Add the upstream theme crate/dependencies and port built-in/imported theme, accent, surface, and persistence tests before wiring appearance UI.
- [ ] 5.2 Port interface font catalog, installed-font filtering, UI size persistence, and remeasurement tests while preserving monospaced code surfaces.
- [ ] 5.3 Port scalable harness-tabbed model picker, connected provider rows, favorites, loading states, forward-jump behavior, and completion popup tests.
- [ ] 5.4 Port conversation-aware sidebar organization/source context and archive/jump shortcut tests, preserving canonical Chat/Session terminology and fork-only sidebar rows.
- [ ] 5.5 Port upstream splash/new-Chat layout polish and official Geist assets only where licensing and fork branding remain correct.

## 6. DOX, Validation, and Handoff

- [ ] 6.1 Update every affected DOX owner/index, `ARCHITECTURE.md`, `CONTEXT.md`, `docs/PARITY.md`, license notices, `fork_changelog.md`, and `fork_sync_report.md`; remove stale contracts.
- [ ] 6.2 Run focused Rust and edge gates continuously, then run one final `cargo test`, `npm -C edge test`, `npm -C edge run typecheck`, `cargo build -p zeron`, `cargo fmt --all -- --check`, packaging checks, strict OpenSpec validation, and `git diff --check`.
- [ ] 6.3 Complete available headed native smoke for appearance, model picker, sidebar shortcuts, transcript, composer completion, and Changes; record unproven visual cases as human-needed.
- [ ] 6.4 Review the final branch diff/report, confirm the worktree is clean and no push/deploy/tag occurred, then leave the retained branch/worktree ready for explicit promotion approval.
