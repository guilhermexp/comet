## Context

See `proposal.md`. Merge produced 138 conflicted files / 1,339 hunks, concentrated in `crates/ui/src/{transcript,shell,composer,pickers}.rs`, `crates/harness/src/opencode/mod.rs` and `Cargo.lock`. Many conflicts are add/add: the fork had already hand-ported upstream features (browser tabs, previews P2P, Cmd+K, appshots, iOS transcript) in a different shape.

## Decisions

### D1. Real merge base via temporary graft

`git merge-base` without help returns `c1a925d9` (2026-08-04) because upstream re-signed every commit. Trees of fork `87123b50` and upstream `b3fa5187` are identical (`a67627a6`), so the merge ran under `git replace --graft b3fa5187 87123b50`; the replace ref was deleted immediately after `git merge --no-commit`. The committed merge has upstream `d721f301` as second parent, so the next sync finds a natural base.

### D2. Conflict policy

Combine both sides hunk by hunk; where incompatible, the fork wins and compatible upstream parts are grafted in. For heavily diverged files (`transcript.rs`, `composer.rs`, `settings.rs`) the fork file was the skeleton and upstream features were ported by symbol.

### D3. Steering stays; queue coexists

Upstream #284 (always queue during a run, remove steering) is rejected, as in the 2026-09-10 adoption. The shared queue (doc/engine RPCs, queue panel, iOS `SessionQueue`) is kept: Enter during a live run steers; the queue panel can still edit, send now or steer now. Upstream's "coalescing steers loses prompts" fix (steers no longer supersede each other) is taken.

### D4. Fork presentation kept

Not taken: upstream compact tool fold (`TOOL_FOLD`), tree tool restyle (#250/#253/#254/#354), Symbols file icons, context ring (`context_usage.rs`, removed as dead code), composer live-preview markdown, in-pill model selector, bottom-docked terminal, New project step navigation (#403), update strip.

### D5. Upstream contracts adapted to fork types

- RPC: the 22 new methods are registered in the fork's single registry (`crates/rpc/src/method.rs`) with upstream forward/stream/deadline rules; `ListModels`/`ListCommands` get upstream's 100 s relay deadline.
- Context usage: the chat doc stores upstream's `{tokens, window}` (`ChatContextUsage`) and converts into the fork's `zeron_proto::ContextUsage` at the transcript boundary.
- Harness installers/skills cover fork-only `HarnessId::Omp` and `HarnessId::Kimi`.
- `executable::find_on_paths` (upstream) replaces the fork resolver and prefers the newest version found.

### D6. Test fallout resolved after the merge

- Code fixes: Codex `subAgentActivity` completion frames close the original chip again (upstream filter was phase-blind); ⌘W outside Workers propagates to the global close so unsaved-file protection and window close run; duplicated `opening` lock removed.
- Test adaptations (fork contract differs): right pane counts as open only with a tab (fork opens Changes on empty toggle); no bottom terminal height; sidebar RPC tests ignore fork-only usage/Workers traffic; Antigravity command test ignores user-global skills; appshot parse hides AX context (upstream presentation, fork transcript already stripped it); upstream's own stale `composer_dock` reduced-motion assertion updated for #453.
- Hermetic tests: `test_shell` uses `WorkersModel::detached` (no daemon poll) under `cfg(test)`.
- macOS 27: `TISCopyCurrentKeyboardLayoutInputSource` aborts off the main thread, which killed the parallel zeron-ui test binary (fork's own loader test too). Vendored `gpui_macos` skips the keyboard-layout query for headless platforms (recorded in `third_party/zui-upstream.toml`); native-metrics tests share `PREVIEW_TEXT_SYSTEM`.

## Risks

- A clean compile does not prove UI parity (no gpui render harness). Visual smoke via `scripts/dev-demo.sh` is required before promotion.
- Auto-merged hunks can re-introduce code both sides added at different places (one duplicated `opening` mutex acquisition — a guaranteed self-deadlock — was caught by an unused-variable warning and fixed). Test suite and headed smoke are the net.
- iOS: `xcodebuild test` not run here.
