# Worker terminal first opening

## Before

Native macOS inspection used `/tmp/CometTerminalQA.app`, a temporary bundle pointing at this checkout's `target/debug/zeron`. The computer-use API could not resolve the unbundled running binary by name, path or `sh.zeron.app`; the temporary bundle made the same local executable accessible without replacing the user's original window.

Opening the existing `.orchestrator` Worker `WTK a3` showed `Loading history…` with older terminal content and overlapping TUI footer rows visibly underneath. A later capture showed the completed response and correctly placed footer. `terminal_panel_bg` uses 0.4 opacity for Glass; the previous overlay therefore could not hide intermediate grids.

## Automated regression evidence

- `/tmp/comet-worker-first-open-red.log`: all three initial regressions failed against the previous implementation (partial grid publication, historical chunk 512, transport failure revealing history).
- `/tmp/comet-worker-first-open-empty-red.log`: the empty read regression failed before its fix.
- The first green focused run passed 33 tests. Final results are recorded below after the complete build.

## Environment interruption

During verification, Cargo's Git database/registry cache and the checkout's `target` were removed externally. One build failed because `target/debug/deps` disappeared during rustc execution. Git dependency caches were restored from the existing local checkouts at the exact locked commits; Cargo then fetched the locked registry packages and rebuilt. No dependency versions or lockfile were changed for this fix.

## Isolation for validation

The main checkout's workspace test encountered E0499 in concurrent, unrelated edits to `crates/engine/src/agent_accounts.rs` (Cursor account lookup). Those edits were left untouched. `/tmp/comet-worker-terminal-qa` is a detached worktree at `bbe8f5c9` with only the corrected `crates/ui/src/workers/terminal.rs` copied in. Both terminal files have SHA-256 `b3d21ddd9288d2b4c5077ae77b91c89bcfc662d3b3a9bdf5c5794319c56d3d60`.

Build and tests use this isolated manifest with `CARGO_TARGET_DIR` pointing at the main checkout's recovered `target` to reuse identical dependency artifacts. The native QA bundle points to that resulting binary. This verifies the terminal change independently of the other unfinished edits; it does not certify their integration.

## Final validation

Native QA of the rebuilt app completed:

- First opening of `.orchestrator` / WTK a3: the first sampled frame (491 ms after the click) contained only the loading indicator and blank terminal background; the next capture showed the final response and a single aligned live footer. These capture times are sampling points, not a benchmark of exact load duration.
- First opening of stopped `deepen-a` Worker: loading indicator without terminal content underneath, followed by its retained final output. The stopped Worker was not restarted.
- Returning to loaded a3 immediately showed the completed view without loading.
- Scrolling up revealed earlier tool output; switching to deepen-a and back preserved the same position and visible content.
- `Scroll to bottom` restored the final a3 response. The corrected native QA window was left there.

The build from the isolated checkout passed (`/tmp/comet-worker-first-open-isolated-build.log`, 3m39s after the cache recovery). The isolated workspace run recorded 2,201 passing tests, one failure and 16 ignored tests before Cargo stopped at UI. All 34 Worker terminal tests passed, including the four new regressions. The sole failure was `workers::settings::tests::detect_cli_default_model_and_formatting`, asserting that the local OMP default label is not `Default`.

The exact settings test was then run on pristine `bbe8f5c9` (the isolated terminal file restored from HEAD) and failed identically: `/tmp/comet-worker-first-open-baseline-settings.log`. The terminal patch was restored afterward and its hash rechecked. No settings code or assertions were changed.

The remaining update/Workers test crates were checked through their 12 already-built test executables from the isolated workspace build, including all ten Workers integration targets: 249 passed, zero failures (`/tmp/comet-worker-first-open-remaining-binaries.log`). This followed an interrupted supplementary Cargo invocation (SIGTERM). Dependency-cache contention was avoided with a private cloned target directory and `RUSTC_WRAPPER=` for the baseline check. The overall workspace run is NOT reported as green, and full workspace doc-tests were not reached by that failed command.

The QA bundle now contains its own copy of the tested binary, SHA-256 `806c79fd253e7c8e13cb21ae3d85db687589717a07904d6ef626e0c0fa0f0979`, so subsequent main-checkout builds cannot replace it. The changed Rust file passes rustfmt and `git diff --check`; the main checkout's whole-workspace formatting check reports only concurrent `cursor_usage.rs` edits. Strict OpenSpec validation passed.
