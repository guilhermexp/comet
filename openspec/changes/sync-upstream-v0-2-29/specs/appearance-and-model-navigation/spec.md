## Purpose

Let users personalize the native interface and navigate large provider catalogs without sacrificing code-surface typography or picker stability.

## ADDED Requirements

### Requirement: Appearance supports durable custom themes and accents

The desktop app SHALL support built-in themes, imported compatible VS Code themes, explicit accents, and surface preferences with durable device-local persistence.

#### Scenario: User imports a valid theme

Test: theme crate unit tests plus headed appearance smoke.

- **WHEN** the user selects a supported VS Code theme file
- **THEN** the app previews and persists a compiled local theme
- **AND** code, transcript, terminal, and surface colors resolve without exposing unsupported source fields

### Requirement: Interface typography is configurable without changing code fonts

The desktop app SHALL offer compatible bundled and installed interface fonts plus a configurable interface size while preserving monospaced code-related surfaces.

#### Scenario: User changes interface font and size

Test: UI settings unit tests plus headed typography smoke.

- **WHEN** the user selects a compatible interface family and size
- **THEN** visible interface text remeasures and rerenders with that selection
- **AND** code blocks, diffs, composer code, and terminal text retain the configured monospaced family

### Requirement: Model selection scales by harness and provider

The model picker SHALL group navigation by runnable harness, scope search to the active harness, identify providers, and remain responsive for thousand-model catalogs.

#### Scenario: Large catalog is searched

Test: UI unit test with a generated thousand-row catalog and literal expected matches.

- **WHEN** the user opens a harness tab and enters a search term
- **THEN** results are limited to that harness and connected providers
- **AND** navigation, favorites, loading rows, and selection remain stable

### Requirement: Composer completion popups use available width

Composer completion popups SHALL use the composer content width and remain scrollable instead of clipping long result lists.

#### Scenario: Completion list exceeds visible height

Test: deterministic composer layout test plus headed smoke.

- **WHEN** a mention or slash completion has more rows than fit vertically
- **THEN** the popup spans the composer content width
- **AND** every result remains reachable by scrolling and keyboard navigation
