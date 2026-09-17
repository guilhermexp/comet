## 1. Implementation

- [x] 1.1 Add subprocess regressions for local compaction usage and unavailable state.
- [x] 1.2 Reuse ordinary OMP completion to refresh local-command context usage.
- [x] 1.3 Run harness tests, update owner documentation and validate/archive OpenSpec.

## Verification

- Before fix: compaction regression failed because no updated context usage was emitted.
- After fix: `cargo test -p zeron-harness` passed (294 tests; 8 live/network tests ignored).
- `cargo fmt --all`, `git diff --check` and strict OpenSpec validation passed.
- No render code changed; native visual QA and a paid live compaction were not run.
