# Composer Corner Radius Design

## Goal

Match the main composer pill to the 12px user-message card radius so sent and pending messages share the same geometry while preserving the input's existing height, spacing, colors, border, shadow, controls, and behavior.

## Scope

- Change only the main composer pill in `crates/ui/src/composer.rs` to 12px.
- Keep the question panel and all other rounded components unchanged.
- Keep `theme.input_glass_bg()`, `theme.border`, the non-frost shadow rule, layout, and interaction states unchanged.

## Validation

- Add a focused unit contract for parity with the 12px message card radius.
- Run the composer tests and workspace checks.
- Restart the development app and visually verify the composer at its current compact state.
