# Tasks

## 1. OpenSpec and TDD

- [x] 1.1 Add failing unit tests in `usage.rs` covering tone boundaries (0, 1, 15, 16, 50, 51, missing), exhausted quota reset badges (>48h and <=48h), and unauthenticated/no-usage states.
- [x] 1.2 Verify test failure with `cargo test -p zeron-ui usage`.

## 2. Usage Derivation and Tones

- [x] 2.1 Implement `UsageTone`, `weekly_usage_tone`, and update `reset_badge_text` and `ProviderUsageRow` in `crates/ui/src/details_sidebar/usage.rs`.
- [x] 2.2 Verify unit tests pass with `cargo test -p zeron-ui usage`.

## 3. Freshness and View Rendering

- [x] 3.1 Implement `usage_snapshot`, `usage_fetched_at`, `usage_tick`, `USAGE_TICK`, and `USAGE_FETCH_INTERVAL` in `crates/ui/src/details_sidebar/view.rs`.
- [x] 3.2 Update `render_usage_row` in `view.rs` to style header summary and reset badge using `weekly_tone`.

## 4. Verification and Closeout

- [x] 4.1 Run `cargo test -p zeron-ui usage`.
- [x] 4.2 Run `cargo build -p comet`.
- [x] 4.3 Run `cargo fmt --all` and verify no new compiler/linter warnings in `crates/ui/src/details_sidebar/`.
- [x] 4.4 Validate OpenSpec change with `openspec validate refresh-usage-and-quota-emphasis --strict --no-interactive`.
- [x] 4.5 Update Usage contract bullet in `crates/ui/AGENTS.md`.
