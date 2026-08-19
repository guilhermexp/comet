# Orchestrator Workers MCP Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give the primary Comet agent a local MCP tool for launching and controlling CLI Workers without granting launch authority to worker sessions.

**Architecture:** `zeron-workers-unpeel` owns an independent stdio MCP server backed by `LocalWorkersClient`. The ACP harness injects that server only for `RunRequest.enable_workers_mcp == true`; primary chat runs enable it while title/discovery/test runs remain disabled. The existing Unpeel MCP server and submodule stay unchanged.

**Tech Stack:** Rust 2024, newline-framed MCP JSON-RPC over stdio, Agent Client Protocol v1 `mcpServers`, `serde_json`, existing Unpeel controller/session-host APIs.

## Global Constraints

- Do not copy or modify Unpeel's GPL MCP host implementation.
- Do not expose `launch_worker` through worker CLI provider configuration.
- Do not expose remove-session or automatic worker termination.
- MCP payloads and waits are bounded; output defaults to ANSI-stripped text.
- The server is cooperative same-UID control, not an OS security boundary.
- Follow RED → GREEN → REFACTOR for every production seam.

---

### Task 1: Controller MCP protocol and startup boundary

**Files:**
- Create: `crates/workers-unpeel/src/controller_mcp.rs`
- Modify: `crates/workers-unpeel/src/lib.rs`
- Test: `crates/workers-unpeel/src/controller_mcp.rs`
- Test: `crates/workers-unpeel/tests/local_actions.rs`

**Interfaces:**
- Produces: `CONTROLLER_MCP_ARG`, `run_stdio()`, `handle_request(Value) -> Option<Value>`, and one `workers` action-enum tool definition.
- Consumes: `COMET_WORKERS_CONTROLLER=1` startup marker.

- [ ] **Step 1: Write failing protocol and startup tests**

```rust
#[test]
fn controller_mode_is_claimed_without_claiming_normal_cli_commands() {
    assert!(is_session_host_mode(&["__workers_mcp__".into()]));
    assert!(!is_session_host_mode(&["workers".into(), "top".into()]));
}

#[test]
fn initialize_and_tools_list_advertise_one_compact_workers_tool() {
    let initialize = handle_request(json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}})).unwrap();
    assert_eq!(initialize["result"]["serverInfo"]["name"], "comet-workers");
    let tools = handle_request(json!({"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}})).unwrap();
    assert_eq!(tools["result"]["tools"].as_array().unwrap().len(), 1);
    assert_eq!(tools["result"]["tools"][0]["name"], "workers");
}
```

- [ ] **Step 2: Run tests and verify RED**

Run: `cargo test -p zeron-workers-unpeel controller_mcp --no-fail-fast`

Expected: compile failure because the controller MCP module and mode do not exist.

- [ ] **Step 3: Implement the minimal MCP loop**

Implement newline-framed stdin/stdout handling for `initialize`, `ping`, `notifications/initialized`, `tools/list`, and `tools/call`. Return JSON-RPC `-32700`, `-32600`, and `-32601` for parse, invalid, and unknown-method errors. Tool-call failures use MCP `isError: true` content. Refuse `run_stdio` unless `COMET_WORKERS_CONTROLLER == "1"`.

- [ ] **Step 4: Run tests and verify GREEN**

Run: `cargo test -p zeron-workers-unpeel controller_mcp --no-fail-fast`

Expected: protocol/startup tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/workers-unpeel/src/controller_mcp.rs crates/workers-unpeel/src/lib.rs crates/workers-unpeel/tests/local_actions.rs
git commit -m "feat(workers): add controller MCP server"
```

### Task 2: Worker orchestration actions

**Files:**
- Modify: `crates/workers-unpeel/src/controller_mcp.rs`
- Test: `crates/workers-unpeel/src/controller_mcp.rs`

**Interfaces:**
- Produces: `ControllerAction`, `dispatch_action(&LocalWorkersClient, &Value)`, bounded key encoding, output cleanup, and wait policy.
- Consumes: `LocalWorkersClient::{bootstrap,launch_session,read_output,transcript_markdown,write,session_action,session_command}`.

- [ ] **Step 1: Write failing action contract tests**

```rust
#[test]
fn launch_requires_exactly_one_launch_mode() {
    assert!(parse_launch(json!({"project_id":"p"})).is_err());
    assert!(parse_launch(json!({"project_id":"p","preset_id":"x","command":"codex"})).is_err());
    assert!(parse_launch(json!({"project_id":"p","preset_id":"x"})).is_ok());
}

#[test]
fn key_encoder_is_bounded_and_deterministic() {
    assert_eq!(encode_keys(&["escape".into(), "down".into(), "enter".into()]).unwrap(), "\u{1b}\u{1b}[B\r");
    assert!(encode_keys(&vec!["enter".into(); 65]).is_err());
    assert!(encode_keys(&["unknown-special".into()]).is_err());
}

#[test]
fn ansi_cleanup_caps_model_output() {
    assert_eq!(clean_output("\u{1b}[31mhello\u{1b}[0m", 1024), "hello");
}
```

- [ ] **Step 2: Run tests and verify RED**

Run: `cargo test -p zeron-workers-unpeel controller_mcp::tests --no-fail-fast`

Expected: failures because action parsing and encoding are absent.

- [ ] **Step 3: Implement actions**

Implement the 13 actions from the design. Validate IDs against the current bootstrap before writes. `read_output` returns at most 64 KiB and transcript at most 96 KiB. `send_text` writes bracketed paste plus carriage return when `submit=true`. `send_keys` accepts at most 64 entries. `wait_for_status` clamps timeout to 1..120 seconds, polls every 250 ms, never mutates the target, and returns the last observed worker on timeout. `archive_worker` resolves the exact session then calls `WorkersSessionCommand::Archive`.

- [ ] **Step 4: Run tests and verify GREEN**

Run: `cargo test -p zeron-workers-unpeel controller_mcp::tests --no-fail-fast`

Expected: all parser/protocol/action tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/workers-unpeel/src/controller_mcp.rs
git commit -m "feat(workers): expose orchestration MCP actions"
```

### Task 3: ACP injection for primary Orchestrator runs

**Files:**
- Modify: `crates/proto/src/agent.rs`
- Modify: `crates/ui/src/composer.rs`
- Modify: `crates/engine/src/doc_host.rs`
- Modify: `crates/engine/src/sessions.rs`
- Modify: `crates/engine/src/titles.rs`
- Modify: `crates/harness/src/acp/mod.rs`
- Modify: all Rust `RunRequest` test literals found by `rg -n 'RunRequest \{'`
- Modify: `crates/harness/tests/fixtures/fake-acp.sh`
- Test: `crates/proto/src/agent.rs`
- Test: `crates/harness/tests/acp.rs`

**Interfaces:**
- Produces: `RunRequest.enable_workers_mcp: bool` and `workers_mcp_servers() -> Vec<Value>`.
- Consumes: ACP v1 `session/new` / `session/load` `mcpServers` array.

- [ ] **Step 1: Write failing wire and ACP tests**

Add tests proving missing JSON defaults the field to false, UI/controller requests set it true, title requests set it false, and an actual harness run sends:

```json
[{"type":"stdio","name":"comet-workers","command":"/absolute/test/zeron","args":["__workers_mcp__"],"env":[{"name":"COMET_WORKERS_CONTROLLER","value":"1"}]}]
```

The fake ACP fixture must still require `mcpServers: []` for discovery probes and must require `comet-workers` for `scenario:workers-mcp`.

- [ ] **Step 2: Run tests and verify RED**

```bash
cargo test -p zeron-proto run_request --no-fail-fast
cargo test -p zeron-harness workers_mcp --no-fail-fast
```

Expected: failures because the field and descriptor builder do not exist.

- [ ] **Step 3: Implement explicit injection**

Add the serde-defaulted field. Set it true in composer and chat-row reconstruction, preserve it through retries/resume, and false for automatic titles and unrelated test helpers. Build the descriptor from `ZERON_WORKERS_MCP_BIN` or `std::env::current_exe()`. Omit it when the request flag is false or `ZERON_DISABLE_WORKERS_MCP=1`. Use the same `session_params` for new/load/fallback.

- [ ] **Step 4: Run tests and verify GREEN**

```bash
cargo test -p zeron-proto run_request --no-fail-fast
cargo test -p zeron-harness workers_mcp --no-fail-fast
cargo test -p zeron-harness --test acp --no-fail-fast
```

Expected: wire and ACP integration tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/proto crates/ui/src/composer.rs crates/engine crates/harness
git commit -m "feat(orchestrator): inject Workers MCP"
```

### Task 4: End-to-end validation and documentation

**Files:**
- Modify: `docs/plans/2026-08-19-orchestrator-workers-mcp-design.md`
- Modify: `docs/research/cmux-resource-management-map.md`
- Test: MCP subprocess and native dev app

**Interfaces:**
- Consumes: completed server and ACP injection.
- Produces: verified Orchestrator-to-Workers lifecycle and operator documentation.

- [ ] **Step 1: Run built-binary stdio smoke**

Pipe initialize, tools/list, and list-projects calls to:

```bash
COMET_WORKERS_CONTROLLER=1 target/debug/zeron __workers_mcp__
```

Assert server name, one tool, and real project IDs. Run once without the marker and assert non-zero exit.

- [ ] **Step 2: Run canonical gates once**

```bash
cargo fmt --all -- --check
cargo test -p zeron-workers-unpeel --no-fail-fast
cargo test -p zeron-proto --no-fail-fast
cargo test -p zeron-harness --no-fail-fast
cargo test -p zeron-engine --no-fail-fast
cargo check -p zeron-ui --no-default-features
cargo build -p zeron
git diff --check
```

Expected: every command exits 0; existing Objective-C `unexpected cfg cargo-clippy` warnings may remain.

- [ ] **Step 3: Validate in the dev app**

Open a fresh primary chat and ask it to list Workers projects/presets, launch a terminal worker running a deterministic shell command, wait for the text, inspect/read output, stop/archive it, and confirm terminal history remains readable. Verify no controller MCP action appears in a worker CLI's own MCP tool list.

- [ ] **Step 4: Update docs and commit**

Record the implemented action list, explicit controller-only injection, test results, and remaining Browser/Computer-domain non-goals.

```bash
git add docs
git commit -m "docs: document Orchestrator Workers MCP"
```

## Self-Review

- Spec coverage: controller-only launch, every approved orchestration action, provider-neutral ACP injection, bounds, errors, full gates, and native validation each have an explicit task.
- Placeholder scan: no implementation step contains TBD/TODO or an unspecified test.
- Type consistency: `enable_workers_mcp`, `CONTROLLER_MCP_ARG`, `workers_mcp_servers`, and the `workers` action tool retain the same names across tasks.
- Intentional gap: Browser MCP and Computer MCP are not injected into the primary Orchestrator in this phase.
