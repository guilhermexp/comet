# Spec Delta

## Purpose

Provide repeatable local evidence of changes in Zeron's rendered text so developers can investigate readability defects without relying on compressed video or changing visual preferences.

## ADDED Requirements

### Requirement: Observe an existing Zeron window
The monitor SHALL capture lossless PNGs from an explicitly selected or unambiguously discovered running Zeron window on macOS, recording window identity, geometry, pixel dimensions and timestamps. It MUST NOT launch an app or modify preferences, providers or renderers.

#### Scenario: Native observation of an existing window
Test: integration — execute the monitor against the already-running app and inspect PNG signature, recorded identity and geometry.
- **WHEN** the developer starts monitoring an existing Zeron window
- **THEN** the monitor captures that window and records native observation metadata
- **AND** it starts no app instance

#### Scenario: Capture target becomes invalid
Test: integration — controlled capture backend reports a missing or non-Zeron target.
- **WHEN** the selected window disappears, belongs to another app or cannot be captured
- **THEN** monitoring reports a failure and preserves partial evidence
- **AND** it does not silently select another app or report healthy observation

### Requirement: Classify sharpness conservatively
The monitor SHALL distinguish unchanged pixels, intensity or contrast changes, geometry or content changes, and spatial blur candidates relative to a reference. It MUST NOT equate reduced raw edge energy with blur or label a stable baseline as proof of clear rendering.

#### Scenario: Spatial blur changes stable text
Test: unit — synthetic fixed glyph-like content passed through Gaussian blur.
- **WHEN** stable text is spatially blurred relative to its reference
- **THEN** the monitor emits a blur candidate with supporting measurements

#### Scenario: Blur combined with reduced contrast
Test: unit — fixed glyph-like content with Gaussian blur followed by affine contrast reduction.
- **WHEN** spatial blur and reduced contrast occur together
- **THEN** the detector preserves a supported blur candidate rather than treating contrast reduction alone as an explanation

#### Scenario: Reduced contrast preserves contours
Test: unit — fixed content with affine intensity and contrast changes.
- **WHEN** only intensity or contrast changes
- **THEN** the monitor classifies contrast rather than spatial blur

#### Scenario: Content or geometry changes
Test: unit — shifted content, replaced glyph-like shapes and changed dimensions.
- **WHEN** content moves, changes or has different dimensions
- **THEN** the monitor records that change separately rather than asserting blur

#### Scenario: Insufficient evidence
Test: unit — identical pixels and flat images.
- **WHEN** the reference is already soft, unchanged or contains no usable edges
- **THEN** the monitor states its relative-observation limitation and does not claim the app is free of blur

### Requirement: Preserve bounded local incident evidence
The monitor SHALL provide live status, manual incident marks, JSONL observations and a local HTML report with before/after PNG evidence. Capture duration and artifact storage MUST be bounded, outputs private and existing output directories protected from overwrite. The offline mode SHALL accept lossless PNG sequences without accessing a running app.

#### Scenario: Developer marks a perceived incident
Test: integration — scripted mark during an offline controlled sequence.
- **WHEN** the developer marks an incident
- **THEN** the report retains the mark, timestamp and available before/after evidence even if no automated candidate was detected

#### Scenario: Offline replay and limits
Test: integration — real CLI replay of PNG fixtures, output collision and storage-limit cases.
- **WHEN** the monitor replays PNG evidence or reaches a configured storage limit
- **THEN** it records the completed observations and termination reason without overwriting prior runs or exceeding its retention policy
