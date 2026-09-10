## Decision
Extend existing reasoning projection and header render. Fixed lifecycle labels replace excerpt labels; any nonempty reasoning remains expandable. Action text uses text_muted, detail text text_faint; errors keep danger colors.

## Testable seams
Reasoning lifecycle projection, short/full body retention, default open and explicit collapse/open. Red/green focused reasoning tests; final UI suite/build and native visual check. Token changes are paint-only and checked visually.

Spiral: port the reference cubic vertices and trim keyframes directly, sample arc lengths once, paint the trimmed stroke through GPUI. Four 0.5s cycles and two 1s cycles use the shared 30fps pulse clock. The spiral is mounted only while active; completed Thought has no SVG/icon. No Lottie/DOM dependency and no per-frame asset decoding. Theme-neutral tint follows the existing icon token.

Latest user refinement: reasoning bodies default open in both active and complete states; arrows always visible for reasoning, preserving manual collapse. Other tool/turn arrows retain hover behavior.
