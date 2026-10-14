## 1. Tree groups
- [x] 1.1 Projection: ordinary tool runs (file edits included) form one group; live trailing group auto-opens.
- [x] 1.2 Render: summary header + tree connectors with reveal; settled collapsed groups skip building children.
- [x] 1.3 File edits render as rows inside the group; diff card opens beneath on click.
- [x] 1.4 Update tests, DOX, build, `cargo test -p zeron-ui`; validate and archive.

## Verification evidence
- `cargo build`: no warnings.
- `cargo test -p zeron-ui`: 2469 + 1 + 11 passed (2 ignored). Old flat-row tests rewritten to the tree contract; new: edits do not split a run, only the trailing live group opens, fold choice beats the live default, connector parts/continuation/reveal under reduced motion, file-row key pruning.
- Native GPUI visual acceptance of connectors, header and the opened diff card: not proven here (no render harness); owner reviews with `cargo run`.
