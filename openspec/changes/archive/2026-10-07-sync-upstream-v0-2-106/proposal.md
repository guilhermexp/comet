## Why

The fork is based on upstream 9e1a1115. The owner authorized importing the complete 15-commit interval through 916cb1cc (v0.2.106), including Codex voice, project grouping, attachment chips, icons, iOS recovery and model catalog fixes. A real merge preserves ancestry for subsequent syncs while adapting upstream implementation to the fork's established contracts.

## What Changes

- Merge upstream through 916cb1ccb1342c2061963a5f6ecf5c977ba46f2c in an isolated worktree based on the owner's committed main b01350d6.
- Import native/local/remote Codex voice and its media/session/render libraries, desktop stage, iOS media and Live Activities, plus standalone helper compatibility with Codex 0.161.
- Import repository identity grouping and filtering, icon/rolling label rendering, composer attachment chips, Claude snapshot normalization and failed iOS Chat-open recovery.
- Adapt integration to preserve Workers/Unpeel, OMP Live, direct Worker notifications, Enter steering, fork navigation and settings, licensing, vendored zui patches and fork release/version gates.
- Document conflict decisions and prove the adapted fork revision without treating upstream CI as fork evidence.

## Capabilities

### New Capabilities
- `codex-live-voice`: host-owned Codex realtime voice with local and remote clients, canonical transcripts and compatible standalone helper devices.

### Modified Capabilities
- `composer-intake`: staged attachment references at the caret, removal/undo and arbitrary external file delivery.
- `projects-settings`: shared Git repository identity across device registrations, retaining per-checkout execution targets.
- `appearance-and-model-navigation`: upstream icon/rolling labels and curated Claude snapshot deduplication.
- `ios-chat-interactions`: failed real Chat opens return to real navigation with an error instead of fixture content.

## Impact

Rust workspace, harness/engine/RPC/doc/proto, desktop UI, shared mobile core and iOS, packaging and vendored zui. Additive wire fields remain compatible with older devices. No upstream push, release tag, automatic installed-app restart or removal of fork features is part of this merge.
