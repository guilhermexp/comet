## Why
Two measurable defects make a dense answer read as disorganized.

1. Inline code chips force a 24px minimum height into a 22px body line box, so
   the leading stutters line by line. Measured on a real answer, the pitch
   inside single paragraphs ranges 21.75–24.75px where it should be a constant
   22.00. The 24px came from MonoCode's `MarkdownCode`
   (`docs/monocode-file-chip-reference.md:5`); the same document records that
   Comet keeps its own prose typography, so that one number needed adapting and
   did not get it.
2. Every pair of blocks is separated by the same 12px, so a heading sits as far
   from the paragraph it introduces as from the one it follows. Sections never
   group, and the answer reads as one undifferentiated stack.

## What Changes
- An inline code chip's minimum height follows the line height of the text it
  sits in, instead of a fixed 24px. Headings, whose line box is taller, are
  unaffected; body text stops stuttering.
- Block spacing becomes a function of the two neighbouring block kinds: a
  heading takes more space above and less below, and a thematic break takes
  more space on both sides. Everything else keeps the 12px gap.
- The rule lives in one function used by all three places that stack blocks —
  the whole-tree renderer, the transcript's split block rows, and the Markdown
  file preview — so the live and settled transcript keep matching to the pixel.

## Capabilities
### Modified Capabilities
- `turn-step-tool-groups`: steady line rhythm and grouped sections in answers.

## Impact
`crates/ui/src/markdown/`, the transcript's row gap and the Markdown preview's
row padding. No parser, projection, wire or agent change.
