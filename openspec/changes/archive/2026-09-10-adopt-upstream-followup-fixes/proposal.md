## Why

The fork still lacks seven upstream correctness improvements identified in the September 10 comparison. Adopt their behavior while preserving the fork's native presentation, steering and device-local preferences.

## What Changes

- R1: fractional native browser geometry and divider hit testing (#304).
- R2: deliberate composer, picker and terminal focus transitions (#302).
- R3: isolated, tool-free and configurable Chat title generation (#303).
- R4: completion notifications based on actual completed turns (#309).
- R5: JavaScript-family, Kotlin and Dockerfile syntax coverage (#230).
- R6: Codex reasoning paragraph boundaries (#250).
- R7: streaming runway, scroll and selection stability (#257, #261, #262, relevant #265).

## Capabilities

### New Capabilities
- `upstream-followup-correctness`: contracts for these seven adaptations and compatibility with existing fork behavior.

### Modified Capabilities

None; existing capabilities retain their presentation and navigation contracts.

## Impact

Rust UI, native WebKit host, harnesses, engine title and completion lifecycle, optional Session metadata, registry/workspace serialization, syntax queries, RPC and device settings. No edge deployment, dependency migration, editor replacement or queue subsystem adoption.
