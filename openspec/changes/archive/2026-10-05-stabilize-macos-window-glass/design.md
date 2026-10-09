# Design

## Context

The active `monocode-dark` variant declares a radius of 24 and tint coverage of 0.91. `window_background_blur_mode` selects WindowServer whenever a radius is declared; this branch currently removes `blurred_view`. MonoCode commit `25dd57e599e33a1878ce7e45a3187f7863b8d74f` instead combines WindowServer blur with a standard AppKit effect view at alpha 0.01, citing transient unfiltered frames in native hover/capture tests. The analogous defect in Metal remains a hypothesis until headed comparison.

## Goals / Non-Goals

**Goals:** retain a stable, nearly transparent support surface; preserve authored parameters; prove native configuration, reuse, and cleanup; preserve existing zui patches and licensing.

**Non-Goals:** alter the desktop, opacity, blur radius, preferences, Metal scheduling, or other platform backends; install or replace the user's running app as part of verification.

## Decisions

- Keep the support view separate from the customized material view. A standard `NSVisualEffectView` uses `UnderWindowBackground`, `BehindWindow`, `Active`, alpha 0.01, and autoresizing under the content. The customized material class strips tint and rewrites blur filters, so reusing it would change the reference behavior.
- Tie support lifetime to the WindowServer blur mode, reusing the view on repeated application and removing it on exit. Preserve existing material and WindowServer radius selection.
- Work in a separate source checkout reconstructed from the exact tracked zui snapshot if the original source checkout is unavailable. Record the source base tree and updated tree in provenance; copy only reviewed changed source files back into the snapshot. Keep all prior local/upstream patches.
- Native regression runs on the main thread using the existing macOS fixture conventions; it checks real AppKit state and production transitions. Pure selector tests remain useful for platform/declared-radius routing. Unit assertions cannot prove that the reported visual waves are eliminated.

## Risks / Trade-offs

- A second effect view may alter native blur/tint slightly → use the reference alpha and leave the authored window radius/tint unchanged; compare headed output.
- A stale or duplicated backing may survive theme switches → native transition regression must cover repeated apply, radius change, material, opaque, and restoration.
- Source-only validation may miss an actual compositor failure → report native lifecycle evidence separately from visual appearance and build evidence; do not claim the waves fixed without observation.

## Migration Plan

No data migration. Update the vendored source and provenance together; reverting the snapshot delta restores the previous renderer behavior.

## Verification evidence

- Native RED: an isolated app bundle found zero standard backing views where one was required. Bare executables were interrupted with SIGTERM during AppKit startup; foreground execution did not resolve that local launch limitation.
- Native GREEN: the same configuration/lifecycle assertions passed after the patch, including a main-loop timer yield, with exit zero and a required PASS marker. CI builds the harness-free executable, launches an isolated bundle with a 60-second timeout, and checks both completion conditions.
- Four blur selector tests and both closing-punctuation rules passed. The source and vendored Git trees match; all prior patches and licenses were retained.
- Independent review approved the implementation without material findings. The frame-source recovery regression and the downstream `cargo build -p zeron` passed through the protected Cargo wrapper. The app build emitted only the existing compact-unwind `__eh_frame` size warning. Formatting, diff checks and strict OpenSpec validation passed.
- Headed before/after WindowServer fixtures were launched as isolated app bundles. Both window-only captures showed a uniform gray backdrop without enough desktop detail to establish that the reported moving waves disappeared. This optical result is inconclusive. No desktop settings, tint, radius, or user preferences were changed, and both fixture processes were cleaned up.
