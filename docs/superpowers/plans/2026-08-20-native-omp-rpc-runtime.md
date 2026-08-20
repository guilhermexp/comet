# Native OMP RPC Runtime Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add the installed Oh My Pi CLI as a separate `OMP` harness that speaks native `rpc-ui`, while leaving the existing Pi/ACP runtime and every other harness unchanged.

**Architecture:** `OmpHarness` owns the OMP JSONL subprocess and translates its correlated commands and structured events into the existing `Harness`/`AgentEvent` contract. The engine keeps ownership of chat persistence, runtime reuse, cancellation, and opaque native-session resume; the existing Workers controller sidecar is exposed to OMP through one RPC host tool.

**Tech Stack:** Rust 2024, Tokio subprocess/stdio, serde/serde_json, GPUI, SwiftUI, OMP RPC protocol v1/v2-compatible JSONL, existing Comet Workers MCP sidecar, Cargo and Xcode tests.

## Global Constraints

- Add `OMP`; do not remove, rename, replace, or change the behavior of `Pi`/`pi-acp` or any existing harness.
- Launch the literal user-installed `omp` executable; do not bundle or auto-install OMP.
- Use `omp --mode rpc-ui --auto-approve --no-extensions`; do not route OMP through ACP.
- Preserve OMP model identity as the composite `<providerId>/<modelId>` and split only at the first `/` when sending `set_model`.
- Honor the current model returned by `get_state` without changing the shared picker default logic.
- Reuse the existing controller MCP sidecar for the OMP `workers` host tool; do not duplicate Workers action logic.
- Never open an OMP-provided URL automatically, and never expose credentials or full secret-bearing diagnostics.
- Bound inbound frames to 8 MiB, outbound frames to 2 MiB, pending requests to 64, request waits to 10 seconds, and the ready handshake to 15 seconds for discovery / 5 seconds for live runs.
- Treat `agent_end` as non-terminal when `isTerminal == false`; emit exactly one terminal `Done` per run.
- Use TDD for every task and commit after each independently green deliverable.

---

## File map

**Create**

- `crates/harness/src/omp/mod.rs` — `Harness` implementation and live run loop.
- `crates/harness/src/omp/protocol.rs` — bounded wire parsing, model/command/state decoding, composite IDs, redaction.
- `crates/harness/src/omp/process.rs` — subprocess, ready handshake, request correlation, event channel, shutdown.
- `crates/harness/src/omp/normalize.rs` — OMP text/reasoning/tool/subagent frame normalization.
- `crates/harness/src/omp/workers_bridge.rs` — controller MCP sidecar to OMP host-tool bridge.
- `crates/harness/tests/omp_rpc.rs` — protocol, discovery, run, resume, input, Workers, steering, completion tests.
- `crates/harness/tests/fixtures/fake-omp-rpc.sh` — deterministic JSONL runtime fixture.
- `crates/harness/tests/fixtures/fake-workers-controller-mcp.sh` — deterministic single-tool MCP sidecar fixture.

**Modify**

- `crates/proto/src/agent.rs` — additive `HarnessId::Omp` wire variant and serialization test.
- `crates/harness/src/lib.rs` — export native OMP module/type and document the driver split.
- `crates/engine/src/registry.rs` — lazy OMP descriptor/factory and registry coverage.
- `crates/engine/src/agent_accounts.rs` — stable `omp` persistence slug only; OMP auth remains CLI-owned.
- `crates/ui/src/pickers.rs` — OMP rail/model-row icon.
- `crates/ui/src/settings/harnesses.rs` — OMP description and executable name.
- `apps/ios/Zeron/Models/HarnessCatalog.swift` — OMP live-catalog label.
- `apps/ios/Zeron/Theme/BrandMarks.swift` — OMP monochrome mark.
- `docs/plans/2026-08-20-native-omp-rpc-runtime-design.md` — record implementation verification after delivery.

**Reference only**

- `/Users/guilhermevarela/Documents/Orchestrator.dev/src/main/lib/omp-runtime/process.ts`
- `/Users/guilhermevarela/Documents/Orchestrator.dev/src/main/lib/omp-runtime/session-manager.ts`
- `/Users/guilhermevarela/Documents/Orchestrator.dev/src/main/lib/omp-runtime/transform.ts`
- `/Users/guilhermevarela/Documents/Orchestrator.dev/src/main/lib/omp-runtime/tool-bridge.ts`

---

### Task 1: Add the OMP wire identity and platform presentation

**Files:**
- Modify: `crates/proto/src/agent.rs`
- Modify: `crates/engine/src/agent_accounts.rs`
- Modify: `crates/ui/src/pickers.rs`
- Modify: `crates/ui/src/settings/harnesses.rs`
- Modify: `apps/ios/Zeron/Models/HarnessCatalog.swift`
- Modify: `apps/ios/Zeron/Theme/BrandMarks.swift`

**Interfaces:**
- Consumes: existing `HarnessId` serde kebab-case contract and `icons::WORKER_OMP`.
- Produces: `HarnessId::Omp`, wire id `"omp"`, desktop/iOS label `OMP`, and an OMP-specific monochrome mark.

- [ ] **Step 1: Write the failing protocol and presentation tests**

Add this assertion to the `HarnessId` serde test in `crates/proto/src/agent.rs`:

```rust
assert_eq!(
    serde_json::to_string(&HarnessId::Omp).unwrap(),
    r#""omp""#,
);
assert_eq!(
    serde_json::from_str::<HarnessId>(r#""omp""#).unwrap(),
    HarnessId::Omp,
);
```

Add a focused picker test in `crates/ui/src/pickers.rs`:

```rust
#[test]
fn omp_uses_its_own_existing_worker_mark() {
    let (path, tint) = harness_brand_icon(HarnessId::Omp);
    assert_eq!(path, crate::icons::WORKER_OMP);
    assert_eq!(tint, None);
}
```

- [ ] **Step 2: Run the tests and verify RED**

Run:

```bash
cargo test -p zeron-proto harness_id
cargo test -p zeron-ui omp_uses_its_own_existing_worker_mark --no-default-features
```

Expected: compilation fails because `HarnessId::Omp` does not exist.

- [ ] **Step 3: Add the identity and exhaustive mappings**

Extend the enum without changing existing discriminants:

```rust
pub enum HarnessId {
    ClaudeCode,
    Codex,
    Cursor,
    Grok,
    Hermes,
    Pi,
    Omp,
    Opencode,
    Mock,
}
```

Add the engine slug:

```rust
HarnessId::Omp => "omp",
```

Add the desktop picker mapping:

```rust
HarnessId::Omp => (crate::icons::WORKER_OMP, None),
```

Add Settings metadata:

```rust
HarnessId::Omp => "Oh My Pi, driven through the installed omp CLI's native RPC mode.",
HarnessId::Omp => "omp",
```

Add iOS live-catalog presentation:

```swift
"omp": "OMP",
```

Declare `BrandMark: Equatable`, then extend it with `case omp`, a `24 × 24` view box, and the already-approved Workers SVG path:

```swift
case .omp:
    return "M2 4h20l-1.5 4H18v9h2.5v3H13v-3h2V8H9v9h2v3H3.5v-3H6V8H3.5L2 4Z"
```

Map `"omp"` to `.omp`; leave the mark monochrome.

- [ ] **Step 4: Run focused tests and compile every exhaustive match**

Run:

```bash
cargo test -p zeron-proto harness_id
cargo test -p zeron-ui omp_uses_its_own_existing_worker_mark --no-default-features
cargo check -p zeron-engine
cargo check -p zeron-ui --no-default-features
```

Expected: all commands pass; existing harness serialization and icons remain unchanged.

- [ ] **Step 5: Commit the additive identity**

```bash
git add crates/proto/src/agent.rs crates/engine/src/agent_accounts.rs crates/ui/src/pickers.rs crates/ui/src/settings/harnesses.rs apps/ios/Zeron/Models/HarnessCatalog.swift apps/ios/Zeron/Theme/BrandMarks.swift
git commit -m "feat: add OMP harness identity"
```

---

### Task 2: Build the bounded OMP JSONL transport

**Files:**
- Create: `crates/harness/src/omp/protocol.rs`
- Create: `crates/harness/src/omp/process.rs`
- Create: `crates/harness/src/omp/mod.rs`
- Create: `crates/harness/tests/omp_rpc.rs`
- Create: `crates/harness/tests/fixtures/fake-omp-rpc.sh`
- Modify: `crates/harness/src/lib.rs`

**Interfaces:**
- Produces: doc-hidden public test seams `OmpProcess::start`, `request`, `send_control`, `take_events`, `shutdown`; `parse_frame`, `serialize_frame`, `sanitize_diagnostic`.
- Consumes later: discovery and live run tasks use the same transport instance.

- [ ] **Step 1: Write failing pure protocol tests**

Add tests for valid, malformed, oversized, and secret-bearing frames:

```rust
#[test]
fn protocol_bounds_and_redacts_frames() {
    let ready = omp::protocol::parse_frame(r#"{"type":"ready"}"#).unwrap();
    assert_eq!(ready["type"], "ready");
    assert!(omp::protocol::parse_frame("not-json").is_err());
    assert!(omp::protocol::parse_frame(&"x".repeat(8 * 1024 * 1024 + 1)).is_err());
    assert_eq!(
        omp::protocol::sanitize_diagnostic("Authorization: Bearer token-secret-123"),
        "Authorization=[redacted]",
    );
}
```

Add a fake-process test:

```rust
#[tokio::test]
async fn process_correlates_out_of_order_responses() {
    let (process, mut events) = start_fake("out-of-order").await;
    let first = process.request(json!({ "type": "get_state" }));
    let second = process.request(json!({ "type": "get_available_models" }));
    let (state, models) = tokio::join!(first, second);
    assert_eq!(state.unwrap()["sessionId"], "s-1");
    assert_eq!(models.unwrap()["models"][0]["id"], "gpt-5.6-sol");
    assert_eq!(events.recv().await.unwrap()["type"], "available_commands_update");
    process.shutdown().await.unwrap();
}
```

- [ ] **Step 2: Run the tests and verify RED**

Run:

```bash
cargo test -p zeron-harness --test omp_rpc protocol_
cargo test -p zeron-harness --test omp_rpc process_
```

Expected: compilation fails because the OMP modules and fixture do not exist.

- [ ] **Step 3: Implement the protocol boundary**

Expose `protocol` and `process` from `omp/mod.rs` with `#[doc(hidden)] pub mod protocol;` and `#[doc(hidden)] pub mod process;` so integration tests can exercise the real boundary without making it part of the supported product API. Create these constants and functions in `protocol.rs`:

```rust
pub const MAX_INBOUND_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_OUTBOUND_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_PENDING_REQUESTS: usize = 64;

pub fn parse_frame(line: &str) -> Result<serde_json::Value, HarnessError>;
pub fn serialize_frame(value: &serde_json::Value) -> Result<String, HarnessError>;
pub fn sanitize_diagnostic(value: &str) -> String;
pub fn response_data(frame: &serde_json::Value, expected_command: &str)
    -> Result<serde_json::Value, HarnessError>;
```

`parse_frame` must reject a frame over 8 MiB, invalid JSON, a non-object, or a missing/non-string `type`. `serialize_frame` must reject output over 2 MiB. `sanitize_diagnostic` must redact bearer tokens, `apiKey`/`token`/`password`/`secret` assignments, private-key blocks, control whitespace, and then cap the result at 512 characters.

- [ ] **Step 4: Implement process startup and correlation**

Expose this focused surface from `process.rs`:

```rust
pub struct OmpLaunch {
    pub executable: std::path::PathBuf,
    pub cwd: std::path::PathBuf,
    pub ephemeral: bool,
    pub env: Option<std::collections::HashMap<String, String>>,
    pub handshake_timeout: std::time::Duration,
    pub request_timeout: std::time::Duration,
}

pub struct OmpProcess;

impl OmpProcess {
    pub async fn start(launch: OmpLaunch) -> Result<Self, HarnessError>;
    pub async fn request(
        &self,
        command: serde_json::Value,
    ) -> Result<serde_json::Value, HarnessError>;
    pub fn send_control(&self, frame: serde_json::Value) -> Result<(), HarnessError>;
    pub fn take_events(
        &self,
    ) -> Result<tokio::sync::mpsc::Receiver<serde_json::Value>, HarnessError>;
    pub async fn shutdown(&self) -> Result<(), HarnessError>;
}
```

The fixed argv is:

```rust
let mut args = vec![
    "--mode", "rpc-ui",
    "--auto-approve",
    "--no-extensions",
    "--cwd", cwd,
];
if launch.ephemeral {
    args.push("--no-session");
}
```

Use one writer task, one line reader, an `Arc<Mutex<HashMap<String, Pending>>>`, monotonically increasing `comet-N` request IDs, command-name response validation, a rolling 2 KiB stderr tail, and graceful stdin close followed by SIGTERM/SIGKILL through the crate's existing shutdown helpers.

When `env` is `None`, preserve the current environment and use `compose_child_path` so Finder-launched macOS builds can resolve both `omp` and the Bun interpreter beside it. When tests inject `env`, pass that map unchanged.

- [ ] **Step 5: Implement the deterministic fake runtime**

The shell fixture must:

- print `{"type":"ready"}` on startup;
- read one JSON command per line;
- echo correlated `response` frames;
- support scenarios through `FAKE_OMP_SCENARIO` for out-of-order responses, malformed output, early exit, timeout, streaming, resume, questions, Workers, subagents, and non-terminal end;
- record received frames to `FAKE_OMP_RECORD` when set.

Keep every scenario local and provider-free.

Add these shared test helpers:

```rust
fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/fake-omp-rpc.sh")
}

fn fake_env(scenario: &str) -> HashMap<String, String> {
    HashMap::from([
        ("FAKE_OMP_SCENARIO".to_owned(), scenario.to_owned()),
    ])
}

async fn start_fake(
    scenario: &str,
) -> (OmpProcess, mpsc::Receiver<Value>) {
    let process = OmpProcess::start(OmpLaunch {
        executable: fixture_path(),
        cwd: std::env::current_dir().unwrap(),
        ephemeral: true,
        env: Some(fake_env(scenario)),
        handshake_timeout: Duration::from_secs(1),
        request_timeout: Duration::from_secs(1),
    })
    .await
    .unwrap();
    let events = process.take_events().unwrap();
    (process, events)
}
```

Mark the OMP fixture executable:

```bash
chmod +x crates/harness/tests/fixtures/fake-omp-rpc.sh
```

- [ ] **Step 6: Run transport tests and verify GREEN**

Run:

```bash
cargo test -p zeron-harness --test omp_rpc protocol_
cargo test -p zeron-harness --test omp_rpc process_
cargo fmt --all -- --check
```

Expected: all transport tests pass; no real OMP process or network call runs.

- [ ] **Step 7: Commit the transport**

```bash
git add crates/harness/src/omp crates/harness/src/lib.rs crates/harness/tests/omp_rpc.rs crates/harness/tests/fixtures/fake-omp-rpc.sh
git commit -m "feat: add bounded OMP RPC transport"
```

---

### Task 3: Discover the live OMP catalog and commands

**Files:**
- Modify: `crates/harness/src/omp/protocol.rs`
- Modify: `crates/harness/src/omp/mod.rs`
- Modify: `crates/harness/tests/omp_rpc.rs`
- Modify: `crates/harness/tests/fixtures/fake-omp-rpc.sh`

**Interfaces:**
- Consumes: `OmpProcess` request correlation.
- Produces: `discover_models_with_launch(OmpLaunch) -> Vec<Model>` and `discover_commands_with_launch(OmpLaunch) -> Vec<SlashCommand>`; `OmpHarness` consumes both in Task 6.

- [ ] **Step 1: Write failing catalog tests**

```rust
#[tokio::test]
async fn catalog_preserves_provider_identity_and_current_model() {
    let models = omp::discover_models_with_launch(fake_discovery_launch("catalog"))
        .await
        .unwrap();
    assert_eq!(models[0].id, "openai-codex/gpt-5.6-sol");
    assert_eq!(models[0].label, "openai-codex/GPT-5.6 Sol");
    assert!(models[0].reasoning_levels.contains(&ReasoningLevel::Max));
    assert!(models.iter().any(|model| model.id == "anthropic/shared"));
    assert!(models.iter().any(|model| model.id == "openai-codex/shared"));
}

#[tokio::test]
async fn commands_are_discovered_from_the_rpc_runtime() {
    let commands = omp::discover_commands_with_launch(fake_discovery_launch("catalog"))
        .await
        .unwrap();
    assert_eq!(commands[0].name, "model");
    assert_eq!(commands[0].input_hint.as_deref(), Some("provider/model"));
}
```

- [ ] **Step 2: Run and verify RED**

Run:

```bash
cargo test -p zeron-harness --test omp_rpc catalog_
cargo test -p zeron-harness --test omp_rpc commands_
```

Expected: tests fail because `OmpHarness::models` and `commands` are not implemented.

- [ ] **Step 3: Implement lossless model decoding**

Add these helpers:

```rust
pub(crate) fn compose_model_id(provider: &str, model: &str) -> String {
    format!("{provider}/{model}")
}

pub(crate) fn split_model_id(value: &str) -> Result<(&str, &str), HarnessError> {
    let (provider, model) = value.split_once('/').ok_or_else(|| {
        HarnessError::Protocol("OMP model id must be <provider>/<model>".into())
    })?;
    if provider.trim().is_empty() || model.trim().is_empty() {
        return Err(HarnessError::Protocol(
            "OMP model id must contain a provider and model".into(),
        ));
    }
    Ok((provider, model))
}
```

Expose the exact discovery seam from `omp/mod.rs` for integration tests and the later `Harness` implementation:

```rust
#[doc(hidden)]
pub async fn discover_models_with_launch(
    launch: OmpLaunch,
) -> Result<Vec<Model>, HarnessError>;

#[doc(hidden)]
pub async fn discover_commands_with_launch(
    launch: OmpLaunch,
) -> Result<Vec<SlashCommand>, HarnessError>;
```

The test helper constructs an ephemeral launch:

```rust
fn fake_discovery_launch(scenario: &str) -> OmpLaunch {
    OmpLaunch {
        executable: fixture_path(),
        cwd: std::env::current_dir().unwrap(),
        ephemeral: true,
        env: Some(fake_env(scenario)),
        handshake_timeout: Duration::from_secs(1),
        request_timeout: Duration::from_secs(1),
    }
}
```

Decode `get_available_models.data.models[]` fields `provider`, `id`, `name`, `reasoning`, and `contextWindow`. Labels must be `provider/name`; reasoning models receive `[Minimal, Low, Medium, High, XHigh, Max]`; non-reasoning models receive an empty ladder. Reject duplicate composite IDs and more than 1,000 rows.

Request `get_state` and `get_available_models` concurrently, then move the exact current `(provider,id)` to index zero without changing the order of the remaining rows.

- [ ] **Step 4: Implement command decoding**

Call `get_available_commands` and map each unique non-empty command to:

```rust
SlashCommand {
    name,
    description: row.description.unwrap_or_default(),
    input_hint: row.input.and_then(|input| input.hint),
}
```

Cap the catalog at 1,000 commands and each string at the protocol's bounded diagnostic limits.

- [ ] **Step 5: Run and verify GREEN**

Run:

```bash
cargo test -p zeron-harness --test omp_rpc catalog_
cargo test -p zeron-harness --test omp_rpc commands_
```

Expected: current OMP model is first, provider collisions remain distinct, and commands pass.

- [ ] **Step 6: Commit discovery**

```bash
git add crates/harness/src/omp crates/harness/tests/omp_rpc.rs crates/harness/tests/fixtures/fake-omp-rpc.sh
git commit -m "feat: discover OMP models and commands"
```

---

### Task 4: Normalize OMP events into the Comet transcript contract

**Files:**
- Create: `crates/harness/src/omp/normalize.rs`
- Modify: `crates/harness/src/omp/mod.rs`
- Modify: `crates/harness/tests/omp_rpc.rs`

**Interfaces:**
- Consumes: raw OMP event `serde_json::Value` frames.
- Produces: doc-hidden public `OmpNormalizer::push(frame) -> Vec<AgentEvent>`, active subagent tracking, and provider failure state.

- [ ] **Step 1: Write failing normalization tests**

Cover text, thinking, tools, host Workers, subagents, and terminal state:

```rust
#[test]
fn normalizer_maps_structured_stream_and_subagents() {
    let mut normalizer = OmpNormalizer::new(HarnessId::Omp, "/repo", "openai-codex/gpt-5.6-sol");
    assert_eq!(
        normalizer.push(json!({
            "type": "message_update",
            "assistantMessageEvent": { "type": "text_delta", "delta": "hello" }
        })),
        vec![AgentEvent::TextDelta { text: "hello".into() }],
    );
    assert_eq!(
        normalizer.push(json!({
            "type": "message_update",
            "assistantMessageEvent": { "type": "thinking_delta", "delta": "checking" }
        })),
        vec![AgentEvent::ReasoningDelta { text: "checking".into() }],
    );
    let subagent = normalizer.push(json!({
        "type": "subagent_lifecycle",
        "payload": {
            "id": "child-1",
            "parentToolCallId": "task-1",
            "status": "running",
            "agent": "explore",
            "sessionFile": "/tmp/child.jsonl"
        }
    }));
    assert!(matches!(subagent.as_slice(), [AgentEvent::Subagent { parent_tool_use_id, .. }] if parent_tool_use_id == "task-1"));
}
```

- [ ] **Step 2: Run and verify RED**

Run: `cargo test -p zeron-harness --test omp_rpc normalizer_`

Expected: compilation fails because `OmpNormalizer` does not exist.

- [ ] **Step 3: Implement text, reasoning, and tool normalization**

Expose `normalize` from `omp/mod.rs` with `#[doc(hidden)] pub mod normalize;` and expose the normalizer type/methods needed by `omp_rpc.rs`.

Implement these mappings:

```text
message_update.text_delta        -> AgentEvent::TextDelta
message_update.thinking_delta    -> AgentEvent::ReasoningDelta
toolcall_end/tool_execution_start -> AgentEvent::ToolCall
tool_execution_end               -> AgentEvent::ToolResult
available_commands_update        -> AgentEvent::AvailableCommands
notice(level=error)               -> AgentEvent::Error
```

Map tool names to existing `ToolCall` variants:

```rust
match name {
    "bash" => ToolCall::Exec { command: string_arg(input, "command") },
    "read" => ToolCall::ReadFile { path: string_arg(input, "path") },
    "write" => ToolCall::WriteFile { path: string_arg(input, "path"), content: input.get("content").and_then(Value::as_str).map(str::to_owned) },
    "edit" => ToolCall::EditFile { path: string_arg(input, "path"), old_string: optional_string(input, "oldText"), new_string: optional_string(input, "newText") },
    "workers" => ToolCall::Mcp { server: "comet-workers".into(), tool: "workers".into(), input: Some(input.clone()) },
    other => ToolCall::Unknown { name: other.to_owned(), input: Some(input.clone()) },
}
```

Cap persisted tool output at 64 KiB; preserve `isError`; extract a single OMP diff into `ToolDiff` when the result provides path/oldText/newText.

- [ ] **Step 4: Implement subagent attribution**

Track `payload.id -> { parentToolCallId, sessionFile, agent }`.

- Running lifecycle emits a nested `SessionStarted`.
- Nested `subagent_event.payload.event` reuses the same text/reasoning/tool normalizers inside `AgentEvent::Subagent`.
- Terminal lifecycle emits nested `Done::Completed` or `Done::Errored`.
- `active_subagents()` returns the current count so the run loop can defer parent completion.

- [ ] **Step 5: Implement terminal-frame classification**

Expose:

```rust
pub(crate) enum AgentEndDisposition {
    Continue,
    Complete,
    Error(String),
}

pub(crate) fn classify_agent_end(&mut self, frame: &Value) -> AgentEndDisposition;
```

Return `Continue` when `isTerminal == false` or any subagent remains active. Inspect the final assistant message for `stopReason == "error"` and a bounded `errorMessage`; otherwise return `Complete`.

- [ ] **Step 6: Run and verify GREEN**

Run: `cargo test -p zeron-harness --test omp_rpc normalizer_`

Expected: all normalized event and lifecycle tests pass.

- [ ] **Step 7: Commit normalization**

```bash
git add crates/harness/src/omp crates/harness/tests/omp_rpc.rs
git commit -m "feat: normalize OMP RPC events"
```

---

### Task 5: Bridge the existing Workers controller as an OMP host tool

**Files:**
- Create: `crates/harness/src/omp/workers_bridge.rs`
- Modify: `crates/harness/src/omp/mod.rs`
- Modify: `crates/harness/tests/omp_rpc.rs`
- Modify: `crates/harness/tests/fixtures/fake-omp-rpc.sh`

**Interfaces:**
- Consumes: `acp::workers_mcp_servers_for`, existing crate-private JSON-RPC client, OMP `set_host_tools` and `host_tool_call`.
- Produces: `WorkersBridge::start`, `definition`, `handle_call`, and `shutdown`.

- [ ] **Step 1: Write failing Workers bridge tests**

```rust
fn fake_workers_controller_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/fake-workers-controller-mcp.sh")
}

#[tokio::test]
async fn workers_host_tool_is_registered_only_when_enabled() {
    let disabled = WorkersBridge::start(WorkersBridgeOptions {
        enabled: false,
        executable: fake_workers_controller_path(),
        parent_chat_id: Some("chat-1".into()),
    })
    .await
    .unwrap();
    assert!(disabled.is_none());

    let enabled = WorkersBridge::start(WorkersBridgeOptions {
        enabled: true,
        executable: fake_workers_controller_path(),
        parent_chat_id: Some("chat-1".into()),
    })
    .await
    .unwrap()
    .unwrap();
    assert_eq!(enabled.definition()["name"], "workers");
    let result = enabled
        .handle_call("omp-call-1", "workers", json!({ "action": "help" }))
        .await;
    assert_eq!(result["type"], "host_tool_result");
    assert_eq!(result["id"], "omp-call-1");
    assert_eq!(result["isError"], false);
}
```

- [ ] **Step 2: Run and verify RED**

Run: `cargo test -p zeron-harness --test omp_rpc workers_host_tool_`

Expected: test fails because no Workers host-tool registration exists.

- [ ] **Step 3: Start the unchanged controller sidecar**

Resolve the controller executable from an injected `OmpHarness::with_workers_mcp_executable` test seam, then `ZERON_WORKERS_MCP_BIN`, then `std::env::current_exe()`. Use `workers_mcp_servers_for` with `request.enable_workers_mcp`, `ZERON_DISABLE_WORKERS_MCP`, and `workers_parent_chat_id`. Parse its single stdio row, spawn that command with its exact args/environment, connect the existing JSON-RPC client, and perform:

```rust
client.request("initialize", json!({
    "protocolVersion": "2024-11-05",
    "capabilities": {},
    "clientInfo": { "name": "comet-omp", "version": env!("CARGO_PKG_VERSION") }
})).await?;
let tools = client.request("tools/list", json!({})).await?;
```

Convert MCP `inputSchema` to OMP `parameters` while preserving name and description.

`WorkersBridgeOptions.executable` is always explicit: production resolution happens in `OmpHarness` in Task 6, while this task's integration test passes `fake_workers_controller_path()` directly.

- [ ] **Step 4: Forward calls and results**

For each OMP frame:

```rust
{
    "type": "host_tool_call",
    "id": request_id,
    "toolName": "workers",
    "arguments": arguments
}
```

send MCP `tools/call { name: "workers", arguments }`, then return:

```rust
json!({
    "type": "host_tool_result",
    "id": request_id,
    "result": { "content": content },
    "isError": is_error,
})
```

Reject unknown tool names, duplicate request ids, more than 64 pending calls, and sidecar failure with a sanitized text result. Terminate the sidecar with the run.

Create `fake-workers-controller-mcp.sh` with the same newline-framed MCP sequence as the production sidecar (`initialize`, `tools/list`, `tools/call`) and mark it executable:

```bash
chmod +x crates/harness/tests/fixtures/fake-workers-controller-mcp.sh
```

Expose `workers_bridge` from `omp/mod.rs` with `#[doc(hidden)] pub mod workers_bridge;` and make only `WorkersBridgeOptions`, `WorkersBridge`, and the four tested methods public for the integration seam.

- [ ] **Step 5: Run and verify GREEN**

Run:

```bash
cargo test -p zeron-harness --test omp_rpc workers_host_tool_
cargo test -p zeron-workers-unpeel controller_mcp
```

Expected: OMP calls reach the unchanged controller MCP contract, while the existing controller suite stays green.

- [ ] **Step 6: Commit the bridge**

```bash
git add crates/harness/src/omp crates/harness/tests/omp_rpc.rs crates/harness/tests/fixtures/fake-omp-rpc.sh crates/harness/tests/fixtures/fake-workers-controller-mcp.sh
git commit -m "feat: expose Workers to OMP RPC"
```

---

### Task 6: Implement the complete OmpHarness run lifecycle

**Files:**
- Modify: `crates/harness/src/omp/mod.rs`
- Modify: `crates/harness/src/omp/process.rs`
- Modify: `crates/harness/tests/omp_rpc.rs`
- Modify: `crates/harness/tests/fixtures/fake-omp-rpc.sh`

**Interfaces:**
- Consumes: `RunRequest`, `RunControls`, `OmpProcess`, `OmpNormalizer`, `WorkersBridge`.
- Produces: `OmpHarness: Harness` with deterministic native completion and step-boundary steering.

- [ ] **Step 1: Write failing end-to-end harness tests**

Cover one test per behavior:

```rust
#[tokio::test]
async fn run_streams_resumes_steers_answers_and_completes_once() {
    let harness = fake_harness("full-run");
    let (controls, steer, interrupt) = controls_with_answer("Yes");
    let mut request = request("hello");
    request.model = Some("openai-codex/gpt-5.6-sol".into());
    request.reasoning = Some(ReasoningLevel::High);
    request.resume = Some("/tmp/omp-session.jsonl".into());
    request.enable_workers_mcp = true;

    let mut stream = harness.run(request, controls).await.unwrap();
    steer.send(SteerMessage { prompt: "next".into(), message_id: Some("m2".into()) }).await.unwrap();

    let events = collect_until_done(&mut stream).await;
    assert_eq!(events.iter().filter(|event| matches!(event, AgentEvent::Done { .. })).count(), 1);
    assert!(events.iter().any(|event| matches!(event, AgentEvent::Steered { .. })));
    assert!(events.iter().any(|event| matches!(event, AgentEvent::InputRequested { .. })));
    interrupt.cancel();
}
```

Define the test helpers beside the test so later scenarios share one exact harness boundary:

```rust
use futures::{StreamExt as _, stream::BoxStream};
use zeron_harness::{CancellationToken, HarnessError, OmpHarness, RunControls, SteerMessage};
use zeron_proto::{AgentEvent, HarnessId, RunRequest, SandboxLevel, UserInputAnswer};

fn fake_harness(scenario: &str) -> OmpHarness {
    OmpHarness::new()
        .with_executable(fixture_path())
        .with_env(fake_env(scenario))
}

fn request(prompt: &str) -> RunRequest {
    RunRequest {
        prompt: prompt.to_owned(),
        harness: Some(HarnessId::Omp),
        model: None,
        reasoning: None,
        model_options: serde_json::Map::new(),
        cwd: std::env::current_dir().unwrap().to_string_lossy().into_owned(),
        sandbox: SandboxLevel::WorkspaceWrite,
        auto_approve: false,
        enable_workers_mcp: false,
        workers_parent_chat_id: None,
        resume: None,
        attachments: Vec::new(),
        worktree: None,
    }
}

fn controls_with_answer(
    label: &'static str,
) -> (RunControls, tokio::sync::mpsc::Sender<SteerMessage>, CancellationToken) {
    let (steer_tx, steer_rx) = tokio::sync::mpsc::channel(8);
    let interrupt = CancellationToken::new();
    let controls = RunControls {
        request_input: Box::new(move |questions| {
            let (tx, rx) = tokio::sync::oneshot::channel();
            let answers = questions
                .into_iter()
                .map(|question| UserInputAnswer {
                    question_id: question.id,
                    labels: vec![label.to_owned()],
                })
                .collect();
            let _ = tx.send(answers);
            rx
        }),
        steering: steer_rx,
        interrupt: interrupt.clone(),
    };
    (controls, steer_tx, interrupt)
}

async fn collect_until_done(
    stream: &mut BoxStream<'static, Result<AgentEvent, HarnessError>>,
) -> Vec<AgentEvent> {
    let mut events = Vec::new();
    while let Some(event) = stream.next().await {
        let event = event.unwrap();
        let done = matches!(event, AgentEvent::Done { .. });
        events.push(event);
        if done {
            break;
        }
    }
    events
}
```

Add separate tests for inline images, `isTerminal:false`, provider error, active subagent deferral, interrupt, timeout, malformed frame, and child exit.

- [ ] **Step 2: Run and verify RED**

Run: `cargo test -p zeron-harness --test omp_rpc run_`

Expected: tests fail because the `Harness` implementation is incomplete.

- [ ] **Step 3: Implement Harness metadata and discovery**

Export `OmpHarness` from `crates/harness/src/lib.rs` with `pub use omp::OmpHarness;`, then implement:

```rust
#[async_trait]
impl Harness for OmpHarness {
    fn id(&self) -> HarnessId { HarnessId::Omp }
    fn display_name(&self) -> &str { "OMP" }
    fn supports_steering(&self) -> bool { true }
    fn steering_mode(&self) -> SteeringMode { SteeringMode::StepBoundary }
    fn reasoning_levels(&self) -> &[ReasoningLevel] {
        &[
            ReasoningLevel::Minimal,
            ReasoningLevel::Low,
            ReasoningLevel::Medium,
            ReasoningLevel::High,
            ReasoningLevel::XHigh,
            ReasoningLevel::Max,
        ]
    }
    fn installed(&self) -> bool { self.resolve_executable().is_some() }
    fn deterministic_turn_end(&self) -> bool { true }
    async fn models(&self) -> Result<Vec<Model>, HarnessError> { self.discover_models().await }
    async fn commands(&self) -> Result<Vec<SlashCommand>, HarnessError> { self.discover_commands().await }
    async fn run(&self, request: RunRequest, controls: RunControls)
        -> Result<BoxStream<'static, Result<AgentEvent, HarnessError>>, HarnessError>;
}
```

Resolve `OMP_EXECUTABLE` first, then `find_on_paths("omp", Vec::new())`; never fall back to `pi`.

Expose deterministic process seams used only by integration tests:

```rust
impl OmpHarness {
    pub fn new() -> Self { Self::default() }

    pub fn with_executable(mut self, executable: impl Into<PathBuf>) -> Self {
        self.executable = Some(executable.into());
        self
    }

    pub fn with_env(
        mut self,
        env: std::collections::HashMap<String, String>,
    ) -> Self {
        self.env = Some(env);
        self
    }

    pub fn with_workers_mcp_executable(
        mut self,
        executable: impl Into<PathBuf>,
    ) -> Self {
        self.workers_mcp_executable = Some(executable.into());
        self
    }
}
```

- [ ] **Step 4: Implement setup and resume**

After `ready`, execute in order:

```rust
process.request(json!({ "type": "set_subagent_subscription", "level": "events" })).await?;
if let Some(session_path) = request.resume.as_deref() {
    process.request(json!({ "type": "switch_session", "sessionPath": session_path })).await?;
}
if let Some(model) = request.model.as_deref() {
    let (provider, model_id) = split_model_id(model)?;
    process.request(json!({ "type": "set_model", "provider": provider, "modelId": model_id })).await?;
}
if let Some(reasoning) = request.reasoning {
    process.request(json!({ "type": "set_thinking_level", "level": reasoning_wire(reasoning) })).await?;
}
```

Start/register `WorkersBridge` only when enabled. Request `get_state`, require a non-empty `sessionFile` or `sessionId`, then emit `SessionStarted` with the opaque resume value.

- [ ] **Step 5: Implement prompt, images, and multiplexed control**

Load PNG/JPEG/GIF/WebP attachments up to 25 MiB each and send:

```rust
json!({
    "type": "prompt",
    "message": request.prompt,
    "images": images,
})
```

The run task uses `tokio::select!` over:

- OMP event frames;
- `controls.steering.recv()` → RPC `steer` + `AgentEvent::Steered`;
- `controls.interrupt.cancelled()` → RPC `abort`, bounded wait, child shutdown, `Done::Interrupted`;
- interactive answer futures → correlated `extension_ui_response`;
- child/process fatal error → `Error` + `Done::Errored`.

- [ ] **Step 6: Implement rpc-ui questions fail-closed**

Map:

```text
select  -> one UserInputQuestion with the supplied options
confirm -> options ["Yes", "No"]
input   -> free-text question with no fixed options
editor  -> free-text question seeded by the visible prefill
```

Send `value`, `confirmed`, or `cancelled:true` using the original OMP request id. Unsupported `open_url`, `setWidget`, `setStatus`, and editor-mutation requests receive `cancelled:true`; no shell or browser action is performed.

- [ ] **Step 7: Implement deterministic completion**

- Ignore `agent_end` with `isTerminal:false`.
- Defer terminal parent end while `normalizer.active_subagents() > 0`.
- On terminal success, refresh `get_state`, emit usage if present, then `Done::Completed { session_id }`.
- On provider failure, emit `Error` then `Done::Errored { error, session_id }`.
- Close OMP and Workers processes after the stream consumer drops or the run terminates.

- [ ] **Step 8: Run lifecycle tests and verify GREEN**

Run:

```bash
cargo test -p zeron-harness --test omp_rpc run_
cargo test -p zeron-harness --test omp_rpc
cargo test -p zeron-harness
```

Expected: every fake-runtime scenario passes and the existing Claude/Codex/Cursor/ACP tests remain green.

- [ ] **Step 9: Commit the complete harness**

```bash
git add crates/harness/src/omp crates/harness/tests/omp_rpc.rs crates/harness/tests/fixtures/fake-omp-rpc.sh
git commit -m "feat: run OMP through native RPC"
```

---

### Task 7: Register OMP without changing existing harness availability

**Files:**
- Modify: `crates/engine/src/registry.rs`
- Modify: `crates/engine/src/agent_accounts.rs`
- Modify: `crates/engine/src/profile.rs` only if a compile-time exhaustive test requires the new wire id.

**Interfaces:**
- Consumes: `zeron_harness::OmpHarness`.
- Produces: lazy `OMP` descriptor in `ListHarnesses`, installed/enabled gating, no account-manager surface.

- [ ] **Step 1: Write failing registry tests**

```rust
#[test]
fn default_registry_adds_omp_without_replacing_pi() {
    let registry = default_registry();
    let ids: Vec<_> = registry.descriptors().into_iter().map(|row| row.id).collect();
    assert!(ids.contains(&HarnessId::Pi));
    assert!(ids.contains(&HarnessId::Omp));
    let pi = ids.iter().position(|id| *id == HarnessId::Pi).unwrap();
    let omp = ids.iter().position(|id| *id == HarnessId::Omp).unwrap();
    assert!(pi < omp);
}
```

Add an executable-seam test with `OmpHarness::with_executable(fake_path)` to assert `installed == true`, `display_name == "OMP"`, and `SteeringMode::StepBoundary`.

- [ ] **Step 2: Run and verify RED**

Run: `cargo test -p zeron-engine registry::tests::default_registry_adds_omp_without_replacing_pi`

Expected: OMP is absent.

- [ ] **Step 3: Add the lazy registry slot after Pi**

```rust
registry.register_lazy(
    HarnessDescriptor {
        id: HarnessId::Omp,
        name: "OMP".into(),
        supports_steering: true,
        steering_mode: SteeringMode::StepBoundary,
        reasoning_levels: vec![
            ReasoningLevel::Minimal,
            ReasoningLevel::Low,
            ReasoningLevel::Medium,
            ReasoningLevel::High,
            ReasoningLevel::XHigh,
            ReasoningLevel::Max,
        ],
        installed: true,
        enabled: None,
    },
    Box::new(|| zeron_harness::OmpHarness::new().installed()),
    Box::new(|| Ok(Arc::new(zeron_harness::OmpHarness::new()) as Arc<dyn Harness>)),
);
```

Do not add OMP to the Accounts provider list; authentication remains in `~/.omp` and the OMP CLI.

- [ ] **Step 4: Run registry and engine regression tests**

Run:

```bash
cargo test -p zeron-engine registry::tests
cargo test -p zeron-engine sessions::tests::live_routing_requires_the_same_runtime_configuration
cargo check -p zeron-engine
```

Expected: OMP appears additively, Pi remains present, and configuration-sensitive restart behavior stays green.

- [ ] **Step 5: Commit registration**

```bash
git add crates/engine/src/registry.rs crates/engine/src/agent_accounts.rs crates/engine/src/profile.rs
git commit -m "feat: register OMP runtime"
```

---

### Task 8: Validate desktop/iOS catalog behavior and the real application

**Files:**
- Modify tests beside: `crates/ui/src/pickers.rs`
- Modify tests beside: `crates/ui/src/settings/harnesses.rs`
- Modify tests beside: `apps/ios/ZeronTests/`
- Modify: `docs/plans/2026-08-20-native-omp-rpc-runtime-design.md`

**Interfaces:**
- Consumes: registered OMP descriptor and live model catalog.
- Produces: proven separate Pi/OMP UI and recorded delivery evidence.

- [ ] **Step 1: Add desktop picker regression coverage**

```rust
#[test]
fn offered_catalog_keeps_pi_and_omp_as_separate_rails() {
    let rows = vec![
        descriptor(HarnessId::Pi, "Pi", Some(true), true),
        descriptor(HarnessId::Omp, "OMP", Some(true), true),
    ];
    assert_eq!(
        offered_harnesses(&rows).into_iter().map(|row| row.id).collect::<Vec<_>>(),
        vec![HarnessId::Pi, HarnessId::Omp],
    );
}
```

- [ ] **Step 2: Add iOS presentation regression coverage**

Assert `HarnessCatalog.label(for: "omp") == "OMP"` and `BrandMark.forHarness("omp") == .omp`; do not add OMP to the iOS static fallback harness list or static model table.

- [ ] **Step 3: Run the automated release gates**

Run:

```bash
cargo fmt --all -- --check
cargo test -p zeron-proto
cargo test -p zeron-harness
cargo test -p zeron-engine registry::tests
cargo test -p zeron-ui pickers --no-default-features
cargo check --workspace
cargo build -p zeron
xcodebuild -project apps/ios/Zeron.xcodeproj -scheme Zeron -sdk iphonesimulator -configuration Debug build CODE_SIGNING_ALLOWED=NO
git diff --check
```

Expected: every command passes. If the known `shell_env::unix::tests::falls_back_when_interactive_attempt_hangs` workspace test fails unchanged, record it separately and prove the OMP-focused suites green; do not describe the full workspace suite as green.

- [ ] **Step 4: Perform the live catalog smoke without a paid turn**

Start the rebuilt app:

```bash
RUST_LOG=warn cargo run -p zeron
```

Verify:

- the model rail contains separate Pi and OMP marks;
- opening OMP lists the live `~/.omp` catalog;
- `openai-codex/GPT-5.6-Sol` is selected initially when OMP reports it as current;
- switching back to Pi shows the unchanged Pi ACP catalog;
- Settings → Agents can independently enable/disable OMP and Pi.

- [ ] **Step 5: Perform one minimal real OMP turn**

Create an OMP chat in a disposable temporary repository and send `Reply with exactly: OMP RPC OK`. Verify streamed text, terminal completion, session resume on one follow-up, interruption, and the Workers host tool's presence without launching a worker. Do not use or mutate the Pi chat.

- [ ] **Step 6: Record verification in the design doc**

Append a dated `## Verification` section containing exact commands, pass/fail outcomes, OMP version, selected composite model, and observed live behavior. State separately whether any existing workspace-wide failure remained.

- [ ] **Step 7: Commit the verified integration**

```bash
git add crates/ui/src/pickers.rs crates/ui/src/settings/harnesses.rs apps/ios/Zeron apps/ios/ZeronTests docs/plans/2026-08-20-native-omp-rpc-runtime-design.md
git commit -m "test: verify native OMP runtime"
```

---

## Self-review checklist

- [ ] Every design requirement maps to at least one task.
- [ ] No task changes the Pi executable, adapter, catalog, model defaults, or stored configuration.
- [ ] OMP discovery and live runs both use RPC, never ACP.
- [ ] Composite provider/model identity survives duplicate model IDs.
- [ ] The existing picker default is fed the current OMP model without a shared-picker change.
- [ ] Workers remains one controller tool backed by the existing MCP sidecar.
- [ ] Interactive requests and URLs fail closed.
- [ ] Non-terminal agent ends and subagent continuation do not settle early.
- [ ] All source changes have a preceding RED test and a focused GREEN command.
- [ ] Final claims distinguish focused gates, workspace gates, and real-app evidence.
