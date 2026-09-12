# Tasks: Add Idle Session Recap

- [x] 1. RPC Protocol Definition
  - [x] 1.1 Add `GENERATE_CHAT_RECAP / GenerateChatRecap` to `crates/rpc/src/method.rs` with `GenerateChatRecapParams` and `GenerateChatRecapReply`.
  - [x] 1.2 Verify `cargo check -p zeron-rpc`.

- [x] 2. Engine Recap Pipeline
  - [x] 2.1 Create `crates/engine/src/recap.rs` implementing `select_recap_transcript`, `build_recap_prompt`, `validate_recap`, and `run_recap_model`.
  - [x] 2.2 Add unit tests for transcript projection, prompt formatting, and validation cleaning in `crates/engine/src/recap.rs`.
  - [x] 2.3 Wire `GenerateChatRecap` handler in `crates/engine/src/rpc.rs`.
  - [x] 2.4 Verify `cargo test -p zeron-engine recap`.

- [ ] 3. UI Policy, Persistence and State
  - [x] 3.1 Create `crates/ui/src/details_sidebar/idle_recap.rs` with `IdleRecapEntry`, `evaluate_idle_recap`, and `prune_idle_recaps`.
  - [ ] 3.2 Add unit tests in `idle_recap.rs` testing all evaluation states and pruning boundaries.
  - [x] 3.3 Add `idle_recaps: HashMap<String, IdleRecapEntry>` to `DetailsSidebarPreferences` in `crates/ui/src/details_sidebar/view.rs`.
  - [ ] 3.4 Extend `Settings` in `crates/ui/src/settings.rs` to persist and load recaps cleanly.

- [x] 4. UI Timer Lifecycle and Widget Rendering
  - [x] 4.1 Wire the idle timer lifecycle in `DetailsSidebar` / active chat view, handling stream / draft / message count changes and dispatching `GenerateChatRecap`.
  - [x] 4.2 Render the `IdleRecapRow` in `crates/ui/src/details_sidebar/view.rs` inside the Workspace card below "Projects worked".
  - [x] 4.3 Verify `cargo check -p zeron-ui` and run tests.

- [x] 5. Validation and Closeout
  - [x] 5.1 Run `cargo fmt --all`.
  - [x] 5.2 Run workspace tests: `cargo test -p zeron-engine -p zeron-ui`.
  - [x] 5.3 Validate OpenSpec change: `openspec validate add-idle-session-recap --strict --no-interactive`.
