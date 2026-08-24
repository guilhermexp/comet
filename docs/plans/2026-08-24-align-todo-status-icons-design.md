# Align Todo Status Icons Design

## Goal

Match the supplied To-dos reference by centering check and current-arrow glyphs
inside the same fixed circular status slot and aligning that slot with row text.

## Decision

Reuse the existing 15 px circle, row height, horizontal inset, colors, and
icons. Make the circle a non-shrinking flex container with both-axis centering.
Avoid per-state offsets or absolute positioning so check and arrow share one
geometry contract across long lists and rounded final rows.

## Verification

A pure geometry contract test pins the slot and glyph sizes plus centered
placement. The real GPUI app is compared against the third supplied screenshot
at the same sidebar state.
