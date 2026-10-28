# Tasks

## 1. Audio

- [x] 1.1 Add audio classification regression and all requested extensions.
- [x] 1.2 Add audio loader regression including streamed files above the byte cap; implement LoadedPreview::Audio.
- [x] 1.3 Add native action coverage and mirror video's native document policy/open path for audio; retain non-macOS fallback.

## 2. Navigation

- [x] 2.1 Add Shell regressions for local rich kinds, Host A preservation, remote and unknown-device owners, owner context and dedupe.
- [x] 2.2 Route local rich formats in the common editor entry point using existing preview registration.
- [x] 2.3 Cover absolute external PDF Chat links without changing checkout context.

## 3. Verification and delivery

- [x] 3.1 Run grouped focused Cargo tests through cargo-verify and cargo fmt --all.
- [x] 3.2 Exercise safe native document smoke and record evidence/limits in .tmp/evidence/preview-rich-kinds/.
- [ ] 3.3 Capture full app file tree and Chat links for PDF, HTML, video, audio, CSV, PNG, SVG and external PDF once provider isolation permits launch (currently blocked by scripts/AGENTS.md).
- [x] 3.4 Update owner DOX, validate OpenSpec and create small commits on fix/preview-rich-kinds-routing without push/merge. Archive only after acceptance is complete.

Evidence: `.tmp/evidence/preview-rich-kinds/acceptance.json`, `verification-results.json` and `native-smoke-display-loop.log`. Markdown: 103 passed; file_preview: 29 passed; Shell: 346 passed; native focus: passed. Standalone classifier red/green observed; UI tests authored first but no separate red Cargo run. Full app screenshots remain notProven under the provider-isolation prohibition in `scripts/AGENTS.md`; the isolated native fixture is not a substitute. Leave this change active until task 3.3 is proved.
