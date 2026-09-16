## 1. Role overrides in the theme crate

- [x] 1.1 Add the grouped optional overrides to `Seeds` (`crates/theme/src/builtins.rs`) covering `hover`, `active`, `border`, `border_strong`, `input`, `cursor`, `diff_hunk` and `terminal.selection`; resolve each in `variant()` as override-or-derived; pass the default group from every existing builtin.
- [x] 1.2 Make `terminal_background` alpha-capable and harden `terminal.foreground` against `flatten(terminal_background, background)` instead of the raw seed.
- [x] 1.3 Add `monocode_fidelity_state_roles_are_neutral`: assert the resolved `monocode-dark` hex for the eight overridden roles and the composer fill derived from `input`, each against a literal expected value from the design table.
- [x] 1.4 Add `monocode_fidelity_non_declaring_variants_are_unchanged`: assert that `dracula`, `nord`, `andromeda` and `zeron-dark` keep their pre-change resolved role colours and their pre-change asset hashes.
- [x] 1.5 Add `monocode_fidelity_terminal_background_keeps_alpha`: assert zero alpha survives resolution and that terminal foreground still meets 4.5:1 against the flattened canvas.

## 2. Reseed monocode-dark

- [x] 2.1 Reseed `monocode_dark()` with the design table values and repin `source` revision to `85a5d03`; keep the 12 base hex, the 16 ANSI slots and the 12 syntax slots exactly as they are.
- [x] 2.2 Update the existing `monocode_dark_seed_fidelity_and_surface_treatment` assertions that the reseed makes stale; delete any assertion that only pinned a derived default.

## 3. Frost parameters per variant

- [x] 3.1 Add serde-defaulted frost alpha, blur radius and flat-shell fields to `ThemeVariant` (`crates/theme/src/lib.rs`) with `skip_serializing_if` so untouched variants serialize identically; declare `0.85`, `24.0` and flat shell on `monocode-dark`.
- [x] 3.2 Read the variant alpha in `Theme::glass()` with `GLASS_ALPHA` as fallback, and the variant blur at the `frosted(...)` call sites in `crates/ui/src/popover.rs` and `crates/ui/src/composer.rs` with their current constants as fallbacks; keep the off-macOS opaque gate intact.
- [x] 3.3 Skip the sidebar `wash(0.05)` in `crates/ui/src/shell.rs` when the active variant declares a flat shell; change exactly that call site.
- [x] 3.4 Add `monocode_glass_follows_variant` in `crates/ui`: assert resolved frost alpha, blur radius and shell fill for `monocode-dark` against literal values, and assert a non-declaring variant keeps `GLASS_ALPHA`, `MENU_BLUR` and the extra wash.

## 4. Verification and documentation

- [x] 4.1 Run the ticket gate, then `cargo test -p zeron-theme` and `cargo test -p zeron-ui --lib theme`, and record literal output.
- [ ] 4.2 Launch the app with `monocode-dark` active and confirm the translucent terminal, the neutral selection and the flat sidebar render as specified; capture evidence or report the unverified item.
- [x] 4.3 Update the owning DOX (`crates/theme/AGENTS.md`, `crates/ui/AGENTS.md`) with the override contract and the frost fields, including the `Test:` matrix rows for the new scenarios.
- [x] 4.4 Update `docs/monocode-design.md`: mark the gaps this change closes, repin the referenced revision, and leave the typography, geometry and tinting gaps recorded as still open.
