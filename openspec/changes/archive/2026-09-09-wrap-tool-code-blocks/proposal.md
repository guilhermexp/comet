# Wrap tool code blocks

## Why
Adjacent expanded command cards touch, and long commands force sideways scrolling that moves short output off screen.

## What Changes
- Add 8px separation after expanded tool payloads.
- Wrap invocation/output text to available width with natural height and only vertical scrolling above 360px.
- Preserve source text, indentation, syntax and copy content; diff renderers retain their own geometry.

## Impact
Native transcript renderer and its owner contract. No execution changes.
