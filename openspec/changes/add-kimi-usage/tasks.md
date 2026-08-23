# Tasks

## Contract and identity

- [ ] Add the Kimi provider identity with serde and exhaustive-match coverage without registering a runnable Orchestrator harness.
- [ ] Extend device-local account snapshots and warnings for a non-switchable managed Kimi account.

## Credential lifecycle

- [ ] Resolve `${KIMI_SHARE_DIR:-~/.kimi}/credentials/kimi-code.json` and reject unsafe/non-regular credential sources.
- [ ] Parse OAuth credentials into redacted values and cover missing, malformed, expired, and valid files.
- [ ] Implement near-expiry refresh through the official OAuth endpoint with cross-process locking, post-lock re-read, atomic `0600` replacement, and no token logging.

## Managed usage

- [ ] Fetch the official Kimi Code `/usages` endpoint with timeout and redacted errors.
- [ ] Parse weekly and rolling limit windows from string or numeric fields; ignore malformed rows independently.
- [ ] Feed Kimi quota windows into the existing device-local `AgentAccountsSnapshot` refresh path.

## UI and documentation

- [ ] Render Kimi after Codex in the Usage widget with existing quota/reset/pace behavior.
- [ ] Add Kimi icon mapping using the existing embedded Worker Kimi brand asset or a dedicated non-duplicated asset.
- [ ] Update root, engine, proto, and UI DOX plus project context and glossary pointers.

## Verification

- [ ] Run focused proto, engine Kimi OAuth/usage, and UI Usage tests.
- [ ] Run `cargo check --workspace`, `cargo fmt --all --check`, and `git diff --check`.
- [ ] Build `zeron`, launch the real GPUI app, and confirm Kimi weekly and 5-hour rows using the authenticated local subscription.
- [ ] Validate and archive the OpenSpec change after the implementation and visual smoke pass.
