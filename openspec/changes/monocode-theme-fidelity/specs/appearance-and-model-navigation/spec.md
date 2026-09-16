## ADDED Requirements

### Requirement: Curated themes own their interaction roles

A curated built-in theme SHALL be able to declare the interaction roles that the theme engine otherwise derives from a generic appearance recipe: hover, active, border, border strong, input, cursor, diff hunk and terminal selection. A declared role SHALL resolve to the declared colour, including its alpha, without further mixing. An undeclared role SHALL keep the derived value, so every variant that declares nothing SHALL resolve exactly as before this change. Contrast hardening SHALL continue to apply to text, muted text, accent and terminal foreground.

#### Scenario: MonoCode selection stays neutral
- Test: unit — `zeron-theme` resolves `monocode-dark` roles against literal expected hex from the design table.
- **WHEN** `monocode-dark` resolves
- **THEN** its active, hover, border, border-strong, input and diff-hunk roles are neutral projections of its own text ink at the declared strengths
- **AND** none of those roles carries the accent hue

#### Scenario: A theme that declares nothing is unaffected
- Test: unit — `zeron-theme` compares resolved colours and asset hashes of the non-declaring variants against their pre-change values.
- **WHEN** every variant that declares no interaction roles resolves
- **THEN** each resolved colour equals the value the generic recipe produced before this change
- **AND** each such variant's asset hash is unchanged

### Requirement: Terminal background may be translucent

A theme SHALL be able to declare a terminal background with alpha below full opacity so the terminal reads the window surface behind it instead of a painted slab. Terminal foreground contrast SHALL be resolved against that background flattened over the theme canvas, never against the translucent value itself.

#### Scenario: Transparent terminal over frost
- Test: unit — `zeron-theme` asserts the declared alpha survives resolution and that terminal foreground keeps 4.5:1 against the flattened canvas.
- **WHEN** `monocode-dark` resolves
- **THEN** its terminal background keeps zero alpha over the canvas hue
- **AND** its terminal foreground still meets the readable contrast ratio measured against the flattened canvas

#### Scenario: Terminal contrast is measured on the flattened canvas
- Test: unit — `zeron-theme` asserts a translucent `terminal_background` whose foreground would pass against the raw seed is rejected against the canvas flattened over `colors.background`.
- **WHEN** validation checks terminal foreground contrast
- **THEN** the background is `terminal.background` blended over the theme canvas
- **AND** a foreground that only passes against the uncomposited seed fails

### Requirement: Frost parameters belong to the theme

A theme SHALL be able to declare its frost alpha, its backdrop blur radius and whether its shell is painted flat. The renderer SHALL use the declared values for window frost, floating-card blur and shell tint. A theme that declares none of them SHALL keep the renderer's current alpha, blur radius and shell wash. Platforms without a blur guarantee SHALL keep their existing opaque behaviour regardless of what the theme declares.

#### Scenario: MonoCode frost matches its reference
- Test: unit — `zeron-ui` asserts resolved frost alpha, blur radius and shell flatness for `monocode-dark` and for a variant that declares none.
- **WHEN** `monocode-dark` is active on a platform with frost
- **THEN** the frost alpha, blur radius and flat shell come from the variant
- **AND** a variant that declares none of them keeps the renderer defaults

#### Scenario: Flat shell adds no second tint
- Test: unit — `zeron-ui` asserts the sidebar column fill equals the shell fill under a flat-shell variant and keeps the extra wash otherwise.
- **WHEN** a variant declaring a flat shell paints the sidebar column
- **THEN** the column fill equals the shell fill with no additional wash
- **AND** a variant that does not declare it keeps the existing extra wash

#### Scenario: Authored input wash survives frost
- Test: unit — `zeron-ui` asserts `monocode-dark` `input_glass_bg` alpha equals the seeded 15/255 under forced frost, and `composer_glass_bg` is half of that.
- **WHEN** `monocode-dark` paints input and composer fills on a frost platform
- **THEN** the input wash keeps its authored alpha
- **AND** the composer fill is half of that alpha

#### Scenario: Composer fill keeps plate coverage without an authored wash
- Test: unit — `zeron-ui` asserts `composer_glass_bg` under forced frost is half of 15/255 on `monocode-dark` and `0.5 × a²` of `input_glass_bg` on `zeron-dark`.
- **WHEN** frost is forced on `monocode-dark` and `zeron-dark`
- **THEN** `monocode-dark` composer alpha is half of the seeded wash
- **AND** `zeron-dark` composer alpha is the historical quadratic coverage of its own input plate

#### Scenario: Reverse video stays opaque on a translucent terminal
- Test: unit — `zeron-ui` feeds SGR 7 to a `monocode-dark` cell and asserts the glyph colour is fully opaque with readable contrast against the cell background.
- **WHEN** a terminal cell with default colours is inverted under `monocode-dark`
- **THEN** the glyph colour is the terminal background flattened over the canvas, at full alpha
- **AND** its contrast against the effective cell background stays readable

#### Scenario: Window blur follows the declared frost radius
- Test: unit — `zeron-ui` `window_blur_monocode_dark_requests_declared_radius` and `window_blur_undeclared_variant_requests_none`.
- **WHEN** `monocode-dark` is active on a platform with frost
- **THEN** the window asks `Some(px(24.0))`
- **AND** a variant that declares no frost radius asks `None`, keeping the AppKit material

### Requirement: Frost parameters are bounded at the registry boundary

A deserialized variant SHALL only install when `frost_alpha` is finite and within 0.0..=1.0 and `frost_blur_radius` is finite and within 0.0..=64.0. Values outside those ranges SHALL produce a blocking structural `ValidationIssue` so `load_editable_family` and `install` refuse the file. After clamping, instance blur (`frost_blur_or`) and context-free blur (`current_frost_blur`) SHALL return the same radius for the same variant.

#### Scenario: Out-of-range frost is rejected
- Test: unit — `zeron-theme` asserts `frostBlurRadius` 100000.0, `-1.0` and `frostAlpha` 2.0 produce blocking structural issues, and `monocode-dark` at 0.85/24.0 does not.
- **WHEN** an editable variant declares frost parameters outside the accepted range
- **THEN** validation emits a blocking structural issue
- **AND** `monocode-dark` with 0.85 and 24.0 produces none
