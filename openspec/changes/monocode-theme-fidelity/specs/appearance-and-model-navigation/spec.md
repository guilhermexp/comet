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
