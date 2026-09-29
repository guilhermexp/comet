# Tasks

## 1. Checkout ↔ Space link in the Workers state

- [ ] 1.1 Add optional `space_id` to `CheckoutIdentity` (serde default, unknown keys preserved) and expose it on `WorkersProject`/`ProjectRow`; verify with `cargo test -p zeron-workers-unpeel --lib project_identity` including a round-trip over a state carrying unknown keys.
- [ ] 1.2 Define `SpaceRegistry` + `SpaceRef` in `crates/workers-unpeel` and an in-memory test implementation; verify unit tests compile against it.

## 2. Registry transports

- [ ] 2.1 Implement the `zeron_rpc` WebSocket `SpaceRegistry` for the `__workers_mcp__` process (list via `WatchSpaces` snapshot + devices, ensure via `Mutate createSpace` with canonical path and dedupe); confirm no dependency cycle with `cargo tree -p zeron-workers-unpeel -i zeron-workers-unpeel`; verify with an integration test against a test engine.
- [ ] 2.2 Pass the engine endpoint to the Workers controller MCP in `crates/harness/src/workers_mcp.rs`; verify the harness unit test asserts the env entry.
- [ ] 2.3 Implement the app-process `SpaceRegistry` over the UI engine connection; verify with a UI unit test using a fake client.

## 3. Add, list and launch through the registry

- [ ] 3.1 Rewrite `add_project` per design D3 (worktree → root Space; principal/non-Git → own Space; idempotent; fails without registry, writes nothing); verify spec scenarios "Adding from Workers creates the Space", "Adding a folder twice", both worktree scenarios and "The engine is unreachable" as integration tests in `crates/workers-unpeel/tests/controller_mcp.rs`.
- [ ] 3.2 Controller `list_projects` returns Spaces with device fields and nested local checkouts; verify "Listing mirrors the chat MCP" and "Every project listing agrees" (chat MCP side via `zeron-mcp` over the same fixture).
- [ ] 3.3 `launch_worker` accepts Space id (principal, on-demand principal registration) or checkout id; remote Space fails pre-spawn naming the device; verify the three launch scenarios plus existing `checkout_identity_recovery` blocker tests still pass.
- [ ] 3.4 Update `crates/workers-unpeel/AGENTS.md` (registry ownership, link, transports, verification commands); verify the commands listed run as written.

## 4. Migration

- [ ] 4.1 Implement the one-time migration (D4): backup, reconcile, link/ensure, pending for evidence-less records, marker; verify migration scenarios (unregistered principal with three worktrees, existing Space reuse, second run no-op with no new backup, record without evidence) and "Existing sessions survive the link" (sessions, order, sort modes unchanged) as integration tests.
- [ ] 4.2 Run migration on app start after engine connection; verify with a UI/app unit test that it runs once and surfaces failure as a visible notice instead of silently skipping.

## 5. Consumers read the unified registry

- [ ] 5.1 Settings → Projects built from Spaces × devices × checkout history (D5), device label, search by device name, remote rows without local actions, rename remote; remove `UNREGISTERED_PRIMARY_NAME`; verify `cargo test -p zeron-ui projects` with updated/added cases for every modified `projects-settings` scenario and the remote-project requirement.
- [ ] 5.2 Shell palette: Workers and Settings "+" create/reuse the Space on the local device; verify `workers-project-picker` scenarios (unit + integration) and update its Test stamps.
- [ ] 5.3 Workers sidebar projection uses local Spaces as base projects; remove synthetic repository containers, keep association-pending; verify `workers-sidebar-context` new scenarios with unit tests in `crates/ui/src/workers/workspace.rs`.
- [ ] 5.4 Source Control authorization resolves Worker checkouts through their Space link; delete the `RegisteredProjects` fallback in `crates/engine/src/rpc.rs`; verify both Source Control scenarios as engine integration tests.
- [ ] 5.5 Composer `@` mentions read Spaces with device; update `openspec/changes/add-project-mentions` and `openspec/changes/add-workers-project-filter` artifacts to consume Space ids; verify the mention unit test and `openspec validate` for both changes.
- [ ] 5.6 Update `crates/ui/AGENTS.md`, `CONTEXT.md` (Project = Space; Checkout; remove "Logical Project"/"Registered Project" duplicates) and `ARCHITECTURE.md`; verify terms match the specs.

## 6. Device retirement

- [ ] 6.1 Registry: `retired_devices` row kind, filtered device listings, ignored upsert for retired ids, Space re-home upsert; update the `Space.device_id` doc comment; verify `cargo test -p zeron-doc` for tombstone, resurrect-ignored and re-home.
- [ ] 6.2 Engine `Mutate retireDevice` with full pre-validation and one mutation batch (Spaces + `SetChatHost` + tombstone); verify every `device-retirement` scenario as engine integration tests, including refusal leaving state unchanged.
- [ ] 6.3 Settings → Devices retire action (eligibility, confirmation naming projects and chat count); chat MCP `list_devices`/`deviceName` honor retirement; verify eligibility unit test and chat MCP test.

## 7. Sessions tab

- [ ] 7.1 Session rows for a project: chats + (local only) Worker sessions of principal and worktrees, live + archived, merged by last activity; parent chat; missing checkout; remote project lists chats only; verify unit tests for each Sessions scenario.
- [ ] 7.2 Side panel beside the list reusing the chat-opened Worker surface and a read-only chat `Transcript` with "Go to chat"; archived Worker replays without restart; verify activation unit tests and a native QA screenshot of the panel open next to the list.

## 8. Integration proof

- [ ] 8.1 Full gates: `cargo test --workspace`, `cargo check --all-targets`, `cargo fmt --check`, `scripts/check-test-stamps.sh unify-project-registry` (if present) and `openspec validate unify-project-registry --strict`.
- [ ] 8.2 Sandbox end-to-end on copies of the owner's `~/.unpeel` and profile store: migration result (three consistent lists, JK grouped, no "Principal not registered"), Sessions tab opens a worker transcript, retirement of the legacy device moves `.orchestrator` and its chats; evidence saved outside the sandbox.
