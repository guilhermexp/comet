# Tasks

## Fasing

| Fase | U-IDs | Seções | Depends on | Audit state | Audited commit | Entrega | UAT mode |
|---|---|---|---|---|---|---|---|
| F1 | K1-K5 | §1-§5 | — | pending | — | Kimi managed quota end-to-end | human-driven |

## 1. Contract and identity

**must_haves:** Kimi is a device-local account/usage identity; the native harness registry remains unchanged; wire encodings are pinned.

- [x] K1 Add the Kimi provider identity with serde and exhaustive-match coverage without registering a runnable Orchestrator harness. files: `crates/proto/src/agent.rs`, `crates/proto/src/entities.rs`, `crates/engine/src/registry.rs`, `crates/ui/src/pickers.rs`, `crates/ui/src/settings/harnesses.rs`. verify: `cargo test -p zeron-proto && cargo test -p zeron-engine registry && cargo test -p zeron-ui harness`.
- [x] K2 Extend device-local account snapshots and warnings for a non-switchable managed Kimi account. files: `crates/engine/src/agent_accounts.rs`, `crates/ui/src/settings/accounts.rs`. verify: `cargo test -p zeron-engine kimi && cargo test -p zeron-ui accounts`.

## 2. Credential lifecycle

**must_haves:** tokens are redacted, refresh is race-safe, failure never corrupts the credential file, and no credential crosses RPC.

- [x] K3 Resolve `${KIMI_SHARE_DIR:-~/.kimi}/credentials/kimi-code.json`, reject unsafe sources, parse redacted OAuth state, and cover missing/malformed/expired/valid files. files: `crates/engine/src/kimi_usage.rs`, `crates/engine/src/lib.rs`. verify: `cargo test -p zeron-engine kimi`.
- [x] K4 Implement near-expiry refresh through the official OAuth endpoint with cross-process locking, post-lock re-read, atomic `0600` replacement, and no token logging. files: `crates/engine/src/kimi_usage.rs`. verify: `cargo test -p zeron-engine kimi`.

## 3. Managed usage

**must_haves:** the official managed endpoint is used; weekly and rolling windows survive independent malformed rows; Moonshot API billing stays excluded.

- [x] K5 Fetch `/usages`, parse string/numeric counters and reset windows, and feed results into the existing device-local snapshot refresh. files: `crates/engine/src/kimi_usage.rs`, `crates/engine/src/agent_accounts.rs`, `crates/proto/src/entities.rs`. verify: `cargo test -p zeron-engine kimi`.

## 4. UI and documentation

**must_haves:** order is Claude, Codex, Kimi; existing quota/pace UI is reused; Kimi brand assets are not duplicated; glossary and DOX agree.

- [x] Render Kimi after Codex with existing quota/reset/pace behavior and the embedded Kimi brand asset. files: `crates/ui/src/details_sidebar/usage.rs`, `crates/ui/src/details_sidebar/view.rs`, `crates/ui/src/icons.rs`, `crates/ui/src/settings/accounts.rs`. verify: `cargo test -p zeron-ui usage`.
- [x] Add the CONTEXT pointer and reconcile root/engine/proto/UI DOX plus project context. files: `AGENTS.md`, `crates/engine/AGENTS.md`, `crates/proto/AGENTS.md`, `crates/ui/AGENTS.md`, `openspec/project.md`. verify: `bash /Users/guilhermevarela/.orchestrator/scripts/openspec-doctor.sh /Users/guilhermevarela/Documents/Projetos/SelfHosting/comet --json`.

## 5. Verification

**must_haves:** focused behavior, workspace typecheck, formatting, real authenticated quota, and the desktop surface are proven before archive.

- [x] Run focused proto, engine Kimi OAuth/usage, and UI Usage tests. files: `crates/proto`, `crates/engine`, `crates/ui`. verify: `cargo test -p zeron-proto && cargo test -p zeron-engine kimi && cargo test -p zeron-ui usage`.
- [x] Run workspace checks. files: `Cargo.toml`, `Cargo.lock`. verify: `cargo check --workspace && cargo fmt --all --check && git diff --check`.
- [ ] Build and visually confirm Kimi weekly and 5-hour rows in the real GPUI app using the authenticated local subscription. files: `apps/zeron`, `crates/ui`. verify: `cargo build -p zeron` plus headed-app smoke.
- [ ] Validate and archive the OpenSpec change after review passes. files: `openspec/changes/add-kimi-usage`. verify: `openspec validate add-kimi-usage --strict --no-interactive`.
