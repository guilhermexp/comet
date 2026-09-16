## Verification — 2026-09-14

- Regression first: extensionless external text test failed before the loader fix.
- `cargo test -p zeron-ui --lib file_preview --quiet`: 27 passed (local external absolute/parent/symlink, binary hex, virtual source loading with nonexistent cwd and cleanup).
- `cargo test -p zeron-ui --lib transcript --quiet`: 139 passed, including file URL and range normalization.
- `cargo build -p zeron --bin zeron --quiet`: passed.
- Native QA: isolated app `/tmp/comet-source-control-qa-96b800ec/Comet Source Control QA.app`; clicked `external-report-link` in Files. The link points outside the checkout to an extensionless file. Pane displayed the exact accented report text with line numbers, confirmed by CUA screenshot.
- Virtual resource async loader exercised using GPUI TestAppContext; a live OMP session was not invoked. Full sidecar retrieval reuses the existing FetchToolBlob RPC.
- Limits: existing text/binary size limits remain. Unknown binaries show a bounded hex representation; directory listings cap at 2000 entries. Remote workspace RPC authorization is unchanged.
