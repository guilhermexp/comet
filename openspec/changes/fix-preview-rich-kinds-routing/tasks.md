# Tasks

## 1. Audio

- [ ] 1.1 Add audio classification regression and all requested extensions.
- [ ] 1.2 Add audio loader regression including streamed files above the byte cap; implement LoadedPreview::Audio.
- [ ] 1.3 Add native action coverage and mirror video's native document policy/open path for audio; retain non-macOS fallback.

## 2. Navigation

- [ ] 2.1 Add Shell regressions for local rich kinds, Host A preservation, remote and unknown-device owners, owner context and dedupe.
- [ ] 2.2 Route local rich formats in the common editor entry point using existing preview registration.
- [ ] 2.3 Cover absolute external PDF Chat links without changing checkout context.

## 3. Verification and delivery

- [ ] 3.1 Run grouped focused Cargo tests through cargo-verify and cargo fmt --all.
- [ ] 3.2 Exercise safe native document smoke and record evidence/limits in .tmp/evidence/preview-rich-kinds/.
- [ ] 3.3 Capture full app file tree and Chat links for PDF, HTML, video, audio, CSV, PNG, SVG and external PDF once provider isolation permits launch (currently blocked by scripts/AGENTS.md).
- [ ] 3.4 Update owner DOX, validate OpenSpec and create small commits on fix/preview-rich-kinds-routing without push/merge. Archive only after acceptance is complete.
