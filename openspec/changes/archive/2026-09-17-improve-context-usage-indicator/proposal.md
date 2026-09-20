## Why
The fork preserves OMP compaction but its context tooltip freezes while open, hides the percentage and rounds tokens. Claude assumes capacity and uses aggregate result usage; Codex loses zero/partial measurements. Adopt upstream 8ee7a622 semantics with additive wire compatibility.

## What Changes
- Live tooltip, visible percentage, exact tokens, explicit unknown/partial states and truthful overflow.
- Runtime-reported Claude capacity and primary prompt usage; Codex partial and zero support.
- Preserve OMP compaction, immediate completion refresh and persisted measurements.

## Capabilities
### Modified Capabilities
- `context-usage-continuity`: accurate partial measurements and reactive presentation.

## Impact
proto, harness, engine, doc and composer; additive metadata preserves old snapshot readers. No provider billing or new polling.
