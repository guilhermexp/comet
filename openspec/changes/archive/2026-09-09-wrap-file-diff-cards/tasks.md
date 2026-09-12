## Tasks
- [x] Update diff layout and owner contracts.
- [x] Validate small, truncated and large fetched diff cards with native fixtures; run UI checks and build.

## Evidence
- `cargo test -p zeron-ui --lib`: 1208 passed.
- `cargo build`: passed.
- Native mock review: compact gutters, short Write/Edit natural height, truncated 80-line Write, expanded final rows 78–80, long indented Unicode/path content wrapping within the code column. No horizontal scroller in either plain or virtualized diff body.
- Width invalidation follows GPUI ListState variable-height layout; no separate native resize assertion recorded.
