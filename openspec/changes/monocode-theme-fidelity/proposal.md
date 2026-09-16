## Why

The built-in `monocode-dark` theme reproduces every hex the theme model can seed, yet the app still does not look like MonoCode. The cause is structural: `variant()` applies one generic dark recipe to all 31 variants, so a curated theme cannot say that its selected state is neutral rather than accent-tinted, that its hairline is its own ink rather than pure white, or that its terminal is transparent. The VS Code importer can already set those roles directly (`crates/theme/src/vscode.rs:674-810`); curated built-ins cannot. Per-surface glass alpha and blur radius have no representation at all, so MonoCode's 85% sidebar over a 24px native blur renders as Comet's 80% over 44px.

## What Changes

- Add optional per-variant overrides for the interaction roles `variant()` derives today: `hover`, `active`, `border`, `border_strong`, `input`, `cursor`, `diff_hunk`, `terminal.selection`. Absent overrides keep the current derivation, so the other 30 variants resolve byte-identically.
- Allow `terminal_background` to carry alpha, and resolve terminal foreground contrast against the flattened canvas instead of a translucent colour.
- Add per-variant glass parameters: frost alpha, backdrop blur radius, and a flag for a shell painted flat (no extra wash over the sidebar column). Unset values keep today's `Theme::GLASS_ALPHA`, `frost::MENU_BLUR` and the existing sidebar wash.
- Reseed `monocode-dark` with the measured MonoCode values: stroke 7%, selection-strong 12%, selection-hover 15%, focus 20%, composer 3% (via a 6% input role halved by `composer_glass_bg`), hunk 5% neutral, accent cursor, transparent terminal background, terminal selection 18%, glass 0.85, blur 24, flat shell.
- Repin the theme's provenance revision from `65fbe78` to `85a5d03`, where upstream named the same ladder (`--color-stroke`, `--color-selection*`).

Not in this change: interface typography and geometry per theme, and runtime hue/saturation/lightness tinting. Both need broad `crates/ui` edits and stay out of scope.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `appearance-and-model-navigation`: curated built-in themes gain authority over derived interaction roles and over their own frost alpha, blur radius and shell flatness; importing and persistence behaviour is unchanged.

## Impact

- `crates/theme/src/builtins.rs`: `Seeds` gains an overrides group; `variant()` consults it; `monocode_dark()` reseeded.
- `crates/theme/src/lib.rs`: `ThemeVariant` gains serde-defaulted glass fields; terminal foreground contrast resolves against a flattened background. Asset hashes of untouched variants must not move, so new fields skip serialization when unset.
- `crates/ui/src/theme.rs`, `crates/ui/src/frost.rs`, `crates/ui/src/shell.rs`: glass alpha, blur radius and the sidebar wash read the variant instead of constants.
- No proto, RPC, engine or persistence change. No new dependency.
