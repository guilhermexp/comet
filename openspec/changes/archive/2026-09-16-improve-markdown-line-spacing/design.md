## Context

The native inline renderer uses flex wrapping and centered alignment. Both the chip minimum height and label leading currently consume the entire 22px prose line. Markdown rows use measured native layout; streaming and settled rows share the renderer.

## Goals / Non-Goals

Give dense prose breathing room without changing font size, horizontal layout, links, selection or code blocks.

## Decisions

Raise MD_LINE_HEIGHT to 24px. Derive chip minimum height and label leading from the supplied line height minus 4px, with 2px vertical margins. The margins retain the full outer line height even on chip-only lines; heading chips keep relative sizing. Wrapped labels retain natural height.

## Risks / Trade-offs

Responses grow vertically. Native QA must cover dense paragraphs, chip-only lines and streaming; existing Markdown/transcript tests cover selection and projection regressions.
