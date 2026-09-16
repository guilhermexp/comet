# Verification

Source reference inspected: MonoCode e7d5623480b217fa443f57572f3a6719cc139a88. Presentation rules recorded in docs/monocode-file-chip-reference.md with attribution.

- Clean temporary worktree at /tmp/comet-file-chip-check, HEAD 8750d947, with only this task's source changes copied in. Other work currently modifies engine/RPC/proto in the main checkout; those changes are excluded from this verification.
- `cargo test -p zeron-ui --lib markdown::`: 86 passed, including inline boxes, filename detection, source coverage, real GPUI TestWindow click dispatch/hit testing and selection-copy grouping.
- `cargo test -p zeron-ui --lib transcript::`: 133 passed.
- `cargo build -p zeron`: passed in the isolated worktree.
- Native mock daemon and UI launched with isolated data under /tmp/comet-file-chip-native; UI bundle /tmp/comet-file-chip-native/Zeron Chips QA.app. Uses ZERON_MOCK_ELEMENTS=1 and checked-in synthetic fixture extensions.
- Native visual inspection could not be completed: cua.getApp returns `Computer Use server error -10005: cgWindowNotFound` for the QA app, installed Zeron, and Finder. No screenshot or visual-equivalence claim is made. Native appearance, tooltip and preview interaction remain to be reviewed; change is not archived while that requirement is pending.

## Streaming label regression — 2026-09-14

Root cause: inline prose/chip fragments all passed index zero to flat_text_element. RowVeil keys by element index, so consecutive fragments repeatedly replaced one another's fade baseline, leaving text transparent until streaming stopped. Fragment indices now derive from parent block and source offset using nested_ix.

- Full Markdown suite: 88 passed in the main checkout.
- Strengthened regression invokes the actual inline renderer repeatedly with RowVeil attached and simulated time: passed; unchanged fragments settle while streaming remains active.
- Strict OpenSpec validation and diff checks passed.
