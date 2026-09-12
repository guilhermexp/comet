# Wrap file diff cards

## Why
Write/Edit cards still scroll sideways and their fixed-height lines and variable-width backgrounds leave content clipped and misaligned.

## What Changes
- Wrap diff text within the card, retaining indentation, syntax, gutters and a full-width background per logical line.
- Use variable-height virtualization for large fetched previews and natural height for smaller previews.
- Keep vertical scrolling, bounded preview heights, lazy fetching and rounded corners.

## Impact
Native file-change cards; command payloads and standalone file viewers remain independent.
