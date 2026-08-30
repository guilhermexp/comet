# Tasks

## Fasing

| Fase | U-IDs | Seções | Depends on | Audit state | Audited commit | Entrega | UAT mode |
|---|---|---|---|---|---|---|---|
| F1 | A1-A6 | §1-§5 | — | implemented | — | Antigravity managed quota end-to-end | human-driven |

## 1. Wire Types and Identity

**must_haves:** `HarnessId::Antigravity` is added with serde kebab-case `"antigravity"`; no runnable harness is registered.

- [x] A1 Add `HarnessId::Antigravity` variant with serde tests and exhaustive pattern coverage. files: `crates/proto/src/agent.rs`, `crates/ui/src/pickers.rs`, `crates/ui/src/settings/harnesses.rs`, `crates/ui/src/settings/accounts.rs`. verify: `cargo test -p zeron-proto`.

## 2. Engine Credential Discovery, Refresh, and Fetcher

**must_haves:** In-memory refresh only; zero credential write-back; mandatory User-Agent header; robust quota bucket parser with window ordering.

- [x] A2 Write unit tests for Antigravity credential discovery, store precedence/change/removal, token refresh, and quota summary parsing (including valid fixtures, disabled buckets, missing remainingFraction, empty groups, and unknown groups). files: `crates/engine/src/antigravity_usage.rs`. verify: `cargo test -p zeron-engine antigravity`.
- [x] A3 Implement `antigravity_usage.rs` and wire into `AgentAccounts::list` in `crates/engine/src/agent_accounts.rs`. files: `crates/engine/src/antigravity_usage.rs`, `crates/engine/src/agent_accounts.rs`, `crates/engine/src/lib.rs`. verify: `cargo test -p zeron-engine antigravity`.

## 3. UI Usage Row and Presentation

**must_haves:** Antigravity appears 4th after Kimi; icon rendered cleanly; window labels distinct and correctly ordered.

- [x] A4 Add `crates/ui/assets/icons/antigravity.svg`, export `ANTIGRAVITY` constant in `crates/ui/src/icons.rs`, wire `usage_provider_icon`, update `provider_usage_rows` in `crates/ui/src/details_sidebar/usage.rs`, and refine label rewrite in `crates/ui/src/details_sidebar/view.rs`. files: `crates/ui/assets/icons/antigravity.svg`, `crates/ui/src/icons.rs`, `crates/ui/src/details_sidebar/usage.rs`, `crates/ui/src/details_sidebar/view.rs`. verify: `cargo test -p zeron-ui usage`.

## 4. Verification and Closeout

**must_haves:** All unit tests pass, binary builds, code is formatted, DOX pass updated in engine and UI.

- [x] A5 Run full verification suite and update DOX contracts in `crates/engine/AGENTS.md` and `crates/ui/AGENTS.md`. files: `crates/engine/AGENTS.md`, `crates/ui/AGENTS.md`. verify: `cargo test -p zeron-engine antigravity && cargo test -p zeron-ui usage && cargo build -p zeron && cargo fmt --all`.
- [x] A6 Remove embedded OAuth client material, accept a complete runtime pair through `COMET_ANTIGRAVITY_CLIENT_ID` and `COMET_ANTIGRAVITY_CLIENT_SECRET`, and keep missing-configuration diagnostics redacted. files: `crates/engine/src/antigravity_usage.rs`, `crates/engine/AGENTS.md`. verify: `cargo test -p zeron-engine antigravity`.
