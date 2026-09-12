## 1. Implementation
- [x] 1.1 Cover payload height and overflow geometry.
- [x] 1.2 Frame invocation and output in one scrollable code surface.
- [x] 1.3 Validate tests, build and native expanded tools.

## Validation
- `cargo test -p zeron-ui --lib`: 1,187 passing tests, including bounded combined payload height and verbatim long invocation regression.
- `cargo build -p zeron`: passed.
- Native GPUI: confirmed short eval and hub invocation/result panels, expansion/collapse alignment, hover-only arrows isolated to their header, and horizontal scrolling to the end of a long shell command. Vertical viewport budget is covered by the unit height test.
- DOX owner and DESIGN updated; unrelated existing working-tree changes preserved.
