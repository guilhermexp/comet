# Composer Corner Radius Design

## Goal

Reduce the main composer pill radius from 26px to 22px so the input reads less like a capsule while preserving its existing height, spacing, colors, border, shadow, controls, and behavior.

## Scope

- Change only the main composer pill in `crates/ui/src/composer.rs`.
- Keep the question panel and all other rounded components unchanged.
- Keep `theme.input_glass_bg()`, `theme.border`, the non-frost shadow rule, layout, and interaction states unchanged.

## Validation

- Add a focused unit contract for the 22px radius.
- Run the composer tests and workspace checks.
- Restart the development app and visually verify the composer at its current compact state.

