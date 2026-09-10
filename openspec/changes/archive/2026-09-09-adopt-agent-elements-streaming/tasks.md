## 1. Native adoption
- [x] 1.1 P1–P2: unify headers and framed command/MCP/generic output; verify label and payload tests plus cargo check.
- [x] 1.2 P3: align file headers and preserve failed file diagnostics; verify projection/height tests and native diff.
- [x] 1.3 P4: consolidate group/reasoning summaries and remove title-only expansion; verify projection tests.
- [x] 1.4 P5: refine tasks/search/subagent presentation, no-op and duplicate-title matching; verify task and lifecycle tests.
- [x] 1.5 P6: align errors/Markdown/user/attachments and dedupe error/image projection; verify focused tests and native rendering.
## 2. Integration
- [x] 2.1 Run cargo test --workspace, cargo build, native mixed-output inspection and record evidence.
- [x] 2.2 Update owning DOX and adoption guide, validate and archive OpenSpec after verification.

Evidence: `/tmp/comet-adopt-workspace.log` (2,542 passing tests before the final Edit presentation adjustments); `/tmp/comet-edit-ui3.log` (1,200 UI tests after the final code changes); `/tmp/comet-edit-build4.log` (build passed). Native isolated mock inspected with commands, mixed activity, file cards, Markdown, task/no-op and failed file labels. Screenshots: `codex-shot-2026-09-09_13-04-30.png` (integrated diffs/activity) and `codex-shot-2026-09-09_13-08-21.png` (latest build/replayed Chat). Review app binary SHA-256 equals checkout build: `dd6beee90abe66134228682fba486323529955d6778e404d6499d81bf98fc0a8`. Visual review is a mock, not provider or cross-device end-to-end proof.

Final native capture: `codex-shot-2026-09-09_13-09-18.png` shows the latest integrated Edit/Write cards, line gutters, hidden resting disclosures and expanded activity/narrative spacing. Pointer checks were skipped when the review app lost foreground focus, to avoid operating another app.
