# Verification

- Before changes: three focused terminal regressions failed (surface background, concurrent resize epoch, stopped alternate-screen recovery); 104 existing tests passed. Logs: `/tmp/comet-worker-recovery-red.log`.
- After changes: 107 terminal-related tests passed. Log: `/tmp/comet-worker-recovery-green.log`.
- Current-checkout `cargo build --locked -j 2 -p zeron` passed. Log: `/tmp/comet-worker-surface-build.log`.
- UI unit suite: 1145 passed, 1 failure in the pre-existing dirty `details_sidebar/usage.rs` weekly-tone test, outside this patch. Log: `/tmp/comet-worker-surface-ui-tests.log`.
- Native QA in `/tmp/CometTerminalQA.app`: shared canvas matches across Orchestrator and selected/stopped Workers. WTK a2 (`df507bea-d87a-45e7-9583-cdf39da2c030`) now exposes its final Claude alternate screen; the journal contains 4,845,983 bytes with no retained prefix removed and ended with DEC 1049l. No Worker was relaunched or archived.
- Scope clarification: the user confirmed the stopped screen returned and identified the resize problem as the shell column divider. Old stopped journals can still contain duplicated footers and cursor-position artifacts when replayed at different widths; they contain no historical geometry events. This limitation is not claimed fixed by the divider change.

- Shell divider regressions: three tests failed before the fix (`/tmp/comet-pane-divider-red.log`); all 76 shell tests passed after the fix (`/tmp/comet-pane-divider-green.log`). They cover pointer tracking with Details open, reverse direction at a compressed limit, and preserving the utility width while resizing Details.
- Updated full UI unit suite: 1148 passed, one existing Usage tone failure (`details_sidebar::usage::tests::weekly_tone_neutral_when_no_usage_or_no_weekly_window`), log `/tmp/comet-pane-divider-ui-tests.log`.
- Current divider build passed (`/tmp/comet-pane-divider-build.log`) and launched in `/tmp/CometDividerQA.app`. Native opening, column composition and divider double-click reset were observed. Repeated CUA `drag` calls did not visibly move either divider, so continuous drag behavior is not verified; keep the change open pending that check.

- User confirmed the shell dividers now behave correctly in CometDividerQA, then supplied a new screenshot showing the terminal staying narrow. The selected stopped OMP journal contains many DEC ?7l sequences: replaying them in a narrow emulator discards text before subsequent growth can recover it.
- Three further regressions were observed failing: local geometry during a pending host resize, alternate-screen narrow/wide round trip, and stopped replay with autowrap disabled. The terminal-focused suite now passes 104 tests (`/tmp/comet-terminal-proportion-green.log`). Stopped replay uses the supported host column ceiling before fitting the completed grid; local geometry is independent of host request serialization.

- Updated root build passed (`/tmp/comet-terminal-proportion-build.log`). SHA-256 of `target/debug/zeron` matches the executable reopened in `/tmp/CometDividerQA.app`. Native narrow → wide → narrow with WTK a3 now preserves the full text and reflows grid-wrapped rows. The original TUI's explicit newlines and box widths remain fixed; filling unused width beyond that original formatting would require a separate semantic document layout. User preference requested before making that product change.
- Final UI unit gate: 1151 passed, one unchanged Usage tone failure; log `/tmp/comet-terminal-proportion-ui-tests.log`. `git diff --check` and strict OpenSpec validation passed.
