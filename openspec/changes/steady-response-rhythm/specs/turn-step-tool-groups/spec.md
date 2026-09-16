## ADDED Requirements

### Requirement: Answer text keeps a steady line rhythm
An inline code chip SHALL NOT change the height of the line it sits in. Its
minimum height SHALL follow the line height of the surrounding text, so that
consecutive lines of one paragraph are evenly spaced whether or not they contain
chips. The chip's horizontal padding, radius, fill, icon and hover behavior
SHALL remain unchanged.

#### Scenario: A paragraph mixes plain and chip-bearing lines
Test: none — the chip binds to its line height by construction, so no unit can
fail; native GPUI visual acceptance.
- **WHEN** a paragraph wraps across lines and only some of them contain inline code
- **THEN** every line of that paragraph has the same height
- **AND** a chip inside a heading still fills the heading's taller line box

### Requirement: Headings group with the content they introduce
Markdown block spacing SHALL depend on the kinds of the two adjacent blocks. A
heading SHALL take more space above it than below it, so it reads as belonging
to the content that follows. A thematic break SHALL take that larger space on
both sides. All other block pairs SHALL keep the standard block gap. The
whole-tree renderer, the transcript's per-block rows and the Markdown file
preview SHALL derive this spacing from one shared rule, so a turn's spacing does
not change when it settles.

#### Scenario: An answer has several sections
Test: unit — block gap rule across block-kind pairs.
- **WHEN** an answer alternates headings, paragraphs and lists
- **THEN** each heading sits closer to the block after it than to the block before it
- **AND** paragraph-to-paragraph and paragraph-to-list spacing is unchanged

#### Scenario: A streaming turn settles
Test: unit — transcript row gap against the whole-tree gap for the same blocks.
- **WHEN** a live message is split into one row per top-level block
- **THEN** the gap between two sibling rows equals the gap the unsplit row used for the same pair
- **AND** a heading whose neighbour is not Markdown still takes its section gap
