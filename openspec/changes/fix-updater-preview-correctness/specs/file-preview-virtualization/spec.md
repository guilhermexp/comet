## ADDED Requirements

### Requirement: Virtualized Code Preview Rendering

`render_code` in `FilePreview` SHALL render code lines using `gpui::uniform_list` rather than instantiating elements for all lines upfront, while preserving exact line geometry, numbering, syntax highlighting, and minimap functionality.

#### Scenario: Line geometry and visual styling preserved

Test: GPUI smoke test via isolated runner / dev-demo (visual QA observed: 100k lines, CJK/accents/emojis, minimap)

- **WHEN** a code file is previewed
- **THEN** only visible lines within the viewport range are instantiated by `uniform_list`
- **AND** each line retains 20px line height, font family, line numbering, and syntax highlight text runs
- **AND** empty lines and empty files render without crashing or geometry distortion

#### Scenario: Horizontal and vertical scrolling with Unconstrained width

Test: GPUI smoke test via isolated runner / dev-demo (visual QA observed: horizontal scroll to tail of widest line 50, vertical scroll to line 100,005)

- **WHEN** lines exceed the viewport width or line count exceeds viewport height
- **THEN** vertical scrolling walks the line index smoothly via uniform list down to the last line
- **AND** horizontal scrolling uses `ListHorizontalSizingBehavior::Unconstrained` with width measured from a representative widest line, preserving tail of long lines without viewport clipping mask
- **AND** the minimap remains visible on the right margin with sampled lines
