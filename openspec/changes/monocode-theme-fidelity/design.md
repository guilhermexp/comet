## Context

`variant()` (`crates/theme/src/builtins.rs:98-170`) resolves 12 seeded hex into 26 `ThemeColors` roles. Sixteen of those roles are computed from a fixed recipe shared by every dark variant: `hover = WHITE 11%`, `active = accent 18%`, `border = WHITE 10%`, `border_strong = WHITE 18%`, `input = raised 72%`, `cursor = text 40%`, `diff_hunk = accent 8%`, `terminal.selection = WHITE 22%`. A curated theme has no way to disagree. The renderer adds a second layer of fixed numbers: `Theme::GLASS_ALPHA = 0.80` (`crates/ui/src/theme.rs:691`), `frost::MENU_BLUR = 44.0` (`crates/ui/src/frost.rs:23`), composer blur `16.0` (`crates/ui/src/composer.rs:8360`) and `wash(0.05)` over the sidebar column (`crates/ui/src/shell.rs:10243-250`).

Measured reference (MonoCode `85a5d03`, hue 240, saturation 0%, canvas lightness 9%, ink 92%): `--color-stroke` = ink 7% (`src/index.css:35`); selection ladder 8/10/12/15/20% (`src/index.css:84-88`); composer fill ink 3% (`src/chrome/Composer.tsx:1299`); hunk ink 5% (`src/surfaces/UnifiedDiffView.tsx:910`); terminal background `#00000000` and cursor `--color-accent` (`src/surfaces/TerminalView.tsx:100-102`); terminal selection white 18% (`src/surfaces/TerminalView.tsx:104`); sidebar alpha 0.85 (`src/index.css:81`); native blur radius 24 (`src-tauri/src/macos.rs:52-54`).

## Goals / Non-Goals

Goals: let a curated variant declare the interaction roles and frost parameters that define its identity; reseed `monocode-dark` from the measured reference; leave every other variant bit-identical.

Non-Goals: typography and geometry per theme; runtime hue/saturation/lightness tinting; chat wallpaper; light MonoCode variant; any change to the VS Code importer's own mapping or hardening.

## Decisions

### Optional overrides, not new mandatory fields

`Seeds` gains one grouped field holding `Option<&str>` per overridable role, and `variant()` resolves each role as `override.map(c).unwrap_or(derived)`. Every other builtin passes the default group. Rationale: the derivation is a sensible default for imported and uncurated themes; only curated ports have measured values to assert. Rejected alternative: changing the derivation constants themselves — that silently restyles Dracula, Nord, Andromeda and the 27 other variants.

Overridable roles are exactly those already present in `ThemeColors` and currently unreachable from a seed: `hover`, `active`, `border`, `border_strong`, `input`, `cursor`, `diff_hunk`, `terminal.selection`. Roles that carry contrast guarantees (`text`, `text_muted`, accent roles, `terminal.foreground`) stay derived and hardened.

### Alpha-carrying seeds

The colour parser already accepts `#rrggbbaa` (`crates/theme/src/lib.rs:90-97,211-252`), so an override expresses "own ink at N%" directly and the renderer composites it over whatever surface it lands on — which is what the reference does and what a flattened hex cannot do over frost.

`terminal_background` becomes alpha-capable for the same reason. `terminal.foreground` currently hardens against the raw seed; with alpha that measurement is meaningless, so it hardens against `flatten(terminal_background, background)`. `monocode-dark` seeds `#17171700`: zero alpha, canvas hue, so any naive flatten lands on the canvas.

### Input at 6% so the composer lands at 3%

`composer_glass_bg()` paints the composer pill, the user bubble and toasts. Authored washes (`input_bg.a < 0.20`) take the real half of that alpha, so seeding `input` at ink 6% lands the composer at ink 3% — the reference value — while plain inputs keep ink 6%, matching the reference's search and picker fills. Seeding 3% directly would halve to 1.5% and read as no fill at all. Plate-type inputs (`a ≥ 0.20`) keep the historical quadratic coverage (`0.5 · a²`) that was calibrated by eye; `Hsla::opacity` multiplies, so a linear `opacity(0.5)` on those plates would densify zeron-dark ~1.92× and light ~3.33×.

### Frost parameters on `ThemeVariant`, serde-defaulted

Three fields: frost alpha, blur radius, flat shell. They live on `ThemeVariant`, not `ThemeColors`, because they are compositing parameters rather than colours. All three are `#[serde(default, skip_serializing_if = …)]` so the serialized form of every variant that does not declare them is unchanged — which matters because `asset_hash` is a SHA-256 over that serialized form (`crates/theme/src/builtins.rs:163-169`). Any hash movement outside `monocode-dark` is a regression, and the spec requires asserting it.

Renderer consumption: `Theme::glass()` reads the variant alpha with `GLASS_ALPHA` as fallback; `frost::frosted` call sites read the variant blur with `MENU_BLUR` and the composer's `16.0` as fallbacks; the sidebar wash in `shell.rs` is skipped when the variant declares a flat shell. The existing platform gate stays: off-macOS frost remains opaque and unblurred no matter what the variant declares.

### Provenance repinned to `85a5d03`

The seed was derived by hand from call sites at `65fbe78`; upstream has since named the same values as tokens. The `source` revision moves to `85a5d03` so the provenance points at the revision the values are now traceable to. This changes only `monocode-dark`'s asset hash, which is expected.

## Reseeded values

Ink is `#ebebeb`; canvas is `#171717`. Alpha bytes are `round(pct × 255)`.

| Role | Reference | Seed | Effective over canvas |
|---|---|---|---|
| `border` | `--color-stroke` ink 7% | `#ebebeb12` | `#262626` |
| `border_strong` | focus ink 20% | `#ebebeb33` | `#414141` |
| `hover` | `--color-selection-hover` ink 15% | `#ebebeb26` | `#373737` |
| `active` | `--color-selection-strong` ink 12% | `#ebebeb1f` | `#313131` |
| `input` | search/picker ink 6%; composer ink 3% after halving | `#ebebeb0f` | `#232323`; composer `#1d1d1d` |
| `diff_hunk` | hunk ink 5% | `#ebebeb0d` | `#222222` |
| `cursor` | `--color-accent` | `#459bf7` | `#459bf7` |
| `terminal_background` | `#00000000` | `#17171700` | canvas shows through |
| `terminal.selection` | white 18% | `#ffffff2e` | `#414141` |
| frost alpha | `--sidebar-opacity` 0.85 | `0.85` | — |
| blur radius | CGS radius 24 | `24.0` | — |
| flat shell | rail is canvas at 85%, never a second tint | `true` | — |

## Risks / Trade-offs

A translucent terminal background is new to the renderer: if any terminal paint path assumes an opaque background, the cells will read wrong. Verify by running the app with the theme active before declaring done; if the path cannot honour alpha, report it as not proven rather than reseeding an opaque approximation.

`crates/ui/src/shell.rs` is heavily modified in the main checkout. This change touches one call site there. The merge back into that WIP is the owner's, and the diff is deliberately one line to keep it trivial.

## Migration Plan

Single wave, no compatibility layer: the overrides group and the frost fields are additive with defaults, so all other variants compile and resolve untouched. No persisted data carries these fields today.

## Open Questions

None.
