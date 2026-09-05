## Why

Comet launches every OMP Session with `--no-extensions`, so user and project extensions never load. The user already accepted ambient extension code execution and needs default OMP discovery so installed extensions (including fusion-harness `/fh-*` commands) become available on new Sessions.

## What Changes

- Stop passing `--no-extensions` on the normal `OmpProcess` Session launch (`--mode rpc-ui --auto-approve --allow-home` stay).
- Keep the skill-scope overlay, approval policy, RPC framing, host Workers bridge, cwd, and session semantics unchanged.
- Do not add a toggle, allowlist, fallback, or explicit `--extension` path.
- Do not expand the extension UI RPC bridge; unknown `extension_ui_request` methods remain ignored.
- Worker-maintenance probe args (`--no-extensions --version`) are not Session launch args and stay out of this change.

## Capabilities

### New Capabilities

- `omp-extension-discovery`: OMP Session processes discover user and project extensions by default.

### Modified Capabilities

None. `omp-live-voice` and `harness-tool-normalization` do not describe launch flags.

## Impact

- `crates/harness/src/omp/process.rs`: drop `--no-extensions` from the Session argv.
- `crates/harness/tests/omp_rpc.rs` and `crates/harness/tests/fixtures/fake-omp-rpc.sh`: assert the launch no longer disables discovery while other flags remain.
- `crates/harness/AGENTS.md`: record that Session launch uses default extension discovery; skill overlay still only scopes user skill roots.
- Installed OMP 18.1.11 (`~/.bun/bin/omp`) is the runtime; no OMP binary, Fusion, skills, MCP, provider, or approval changes.
- Existing Chats keep the old child until the host restarts Comet; this change does not restart the running app.
