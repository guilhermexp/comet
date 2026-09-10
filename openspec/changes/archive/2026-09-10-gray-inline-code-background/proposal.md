## Why

Gray inline code blends into surrounding Chat text. The user wants the subtle rounded background shown in their reference when the Gray accent is selected.

## What Changes

- Paint the theme's code wash behind inline code only for the explicit Gray accent.
- Keep text, wrapping, selection, links and other accent presets unchanged.

## Impact

- Native Markdown renderer in `crates/ui/src/markdown/render.rs`.
- Capability: `appearance-and-model-navigation`.
