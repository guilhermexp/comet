# Verification

## Observed defect

The user showed Worker `WT-20261008-jk-checklist-nao-aprovado-sai-do-bloco-de-atencao` with Gemini in Details while its terminal footer showed Opus 5.5.

Read-only inspection of that exact Worker's state captured:

- A provider marker captured at `1791473310620`, pointing to a nested `PgliteSqlCheck.jsonl` provider conversation (`01a11c12-1869-714c-b940-7b519644e992`). Its bound telemetry reported `google-antigravity/gemini-3.8-flash`, 6,601,859 tokens.
- The primary sibling JSONL declared provider conversation `01a11bf5-7b23-7745-bb65-bca313f1ca46` and all 191 assistant records at that read used `anthropic/claude-opus-5-5`, with xhigh thinking.
- A subsequent independent read found the marker replaced by the primary conversation at `1791474004.6`, with Opus:xhigh and 48,630,319 tokens. The two snapshots document intermittent overwrite and later restoration, rather than a permanent parser disagreement.

Only structural provider/model/usage metadata was inspected. The installed application and real Workers were not restarted or messaged. Initial inspection of Worker `172dd48b…` referred to the transmission Worker, not this checklist Worker; those findings are not used to establish this defect.

## Verification cost from the preceding launch-age change

The 22 focused UI tests passed in 0.01 seconds. Compilation dominated: an initial build failed, its protected temporary target was removed, the corrected test build took 26m41s, and the separate normal native build took 10m41s. No Cargo process was active at the current diagnosis preflight. This correction therefore uses executable lifecycle and focused Worker ingress checks in one protected round, without rebuilding gpui.

## Current change checks

- Executable Bun regression against the original rendered extension emitted 11 events, including the nested child's Start, Stop and attention events; six primary-only events were expected.
- The corrected rendered extension emitted exactly six primary events: Start, PermissionRequest, UserPromptSubmit, Stop, Start, Stop. Persisted nested callbacks, a symlink alias and a provider-declared in-memory subagent at depth zero were suppressed; primary default/explicit-root and rebinding callbacks remained available.
- Independent review found no remaining blocking findings after corrections for runtime-specific asset isolation, exact argv migration, explicit-primary precedence, mixed-listener veto and asynchronous marker ordering. The reviewer did not run Cargo independently.
- `bun run --cwd "$PWD/third_party/unpeel" check:runtimes` passed. It first exposed pre-existing tracked Swift projection drift; regenerating that file from unchanged runtime descriptors restored attention reliability for Pi/OMP/Prime and OMP transcript capability. No runtime descriptor changed.
- A first protected Cargo invocation rejected `cargo test -p unpeel-core` before compilation because the dependency is not a member of the root workspace. The corrected single protected round uses `--manifest-path third_party/unpeel/crates/Cargo.toml` for those filters, then the root workspace for Worker tests.
- Core compilation completed in 2m08s. Focused core results: lifecycle extension 9 passed (0.85s); exact extension argument migration 3 passed (0.00s); telemetry 20 passed (0.50s); runtime catalog 24 passed (0.06s); hook assets 52 passed (11.84s). Hook assets also reported two existing ignored tests requiring real authenticated Claude/OpenCode CLIs; this change did not add those ignores or run live provider calls.
- Downstream Worker compilation completed in 11m45s. Activity bridge: 55 passed (2.30s), including old nested Gemini binding repaired by primary Opus evidence, rejected child Stop/attention, mixed-listener veto, offline fallback and asynchronous marker ordering. Session event journal: 9 passed (1.72s), including non-mutating validation, primary-role override and child rejection. Hook migration: 6 passed (0.02s), after 22.07s compiling its separate test target. The protected round exited 0 and removed its disposable target. Across the eight filtered invocations, test execution totaled approximately 17 seconds; compilation totaled approximately 14 minutes. No gpui/native application rebuild was part of this verification.
- The compiler reports a non-blocking unused `new_path` parameter in the shared exact extension-argument replacement helper, repeated for its runtime aliases. The helper uses the already shell-quoted replacement argument. Source remains unchanged after compilation; no extra cold build was added for this cosmetic warning.
- Owning Worker, third-party and nearest Unpeel DOX documents were updated. Vendor provenance matches tree hash `7c917b141d5180d29e29b0c609e6d734ed8704f9`. Scoped rustfmt and diff checks passed. `graft build` refreshed the graph successfully (9 files reparsed, 1605 cached).
- Both complete ADDED requirement blocks were appended to their existing canonical specs without altering prior requirements. Strict change validation and all 62 canonical spec validations passed.

### Reproduction command

Run the following as one serial protected batch from the Comet checkout:

```sh
scripts/cargo-verify.py -- bash -c '
  cargo test --manifest-path third_party/unpeel/crates/Cargo.toml -p unpeel-core --lib lifecycle_extension_ -- --test-threads=1 &&
  cargo test --manifest-path third_party/unpeel/crates/Cargo.toml -p unpeel-core --lib migrates_only_the_exact_extension_argument_value -- --test-threads=1 &&
  cargo test --manifest-path third_party/unpeel/crates/Cargo.toml -p unpeel-core --lib telemetry -- --test-threads=1 &&
  cargo test --manifest-path third_party/unpeel/crates/Cargo.toml -p unpeel-core --lib runtime_catalog -- --test-threads=1 &&
  cargo test --manifest-path third_party/unpeel/crates/Cargo.toml -p unpeel-core --lib hook_assets -- --test-threads=1 &&
  cargo test -p zeron-workers-unpeel --lib activity_bridge -- --test-threads=1 &&
  cargo test -p zeron-workers-unpeel --lib session_event_journal -- --test-threads=1 &&
  cargo test -p zeron-workers-unpeel --test hook_migration -- --test-threads=1'
```

## Deployment limits

The installed release in `/Applications/Zeron.app` was not replaced during verification. A subsequent user-requested `cargo run` compiled the development application in 42.06 seconds using the persistent checkout target, started the engine against the real `/Users/guilhermevarela/.zeron` data directory and visibly loaded the user's existing Chats and devices. This proves normal development startup, not a new live-provider regression test. Its migration path updates managed assets when the corrected application starts; valid primary evidence then repairs an old nested binding. Existing journal records without provider identity/path cannot be retroactively classified. An old loaded extension and old session host with no updated listener online retain legacy offline delivery. No real Worker was interrupted or messaged during this verification.
