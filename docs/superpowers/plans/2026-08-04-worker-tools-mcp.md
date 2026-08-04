# Worker Tools over MCP Implementation Plan

> **For agentic workers:** Steps use checkbox (`- [ ]`) syntax for tracking. Tick each one as you finish it, and commit at the end of every task.

**Goal:** Let the agent in a comet session spawn CLI workers, read them, wait for an exit code, and kill them — through four MCP tools backed by the terminal RPCs the engine already has.

**Design:** `docs/plans/2026-08-04-worker-tools-mcp-design.md`. Read it first; it explains why each bound exists. This plan does not repeat the rationale.

**Architecture:** A new `comet-mcp` crate holds the tool logic behind an `EngineClient` trait, so the four tools are unit-testable without a socket. `comet mcp-server` is a subcommand of the existing binary that wires `rmcp` over stdio to a real `RpcClient`. The harness passes that subcommand to the agent through `--mcp-config` (Claude) and `-c mcp_servers.*` (Codex).

**Tech Stack:** Rust 2024, tokio, `rmcp` 3.1, `comet-rpc`'s existing `connect_ws`/`RpcClient`, Cargo tests.

## Global Constraints

- No change to the engine's RPC surface, the command ledger, the doc schema, or the UI.
- `RunRequest` is persisted verbatim in the CRDT ledger — do not add fields to it.
- No new workspace dependency beyond `rmcp`.
- `--strict-mcp-config` is never passed: the comet server is additive to the user's own MCP servers.
- Every tool error is returned to the agent as a readable tool error, never a panic.
- Follow the workspace formatting: run `cargo fmt --all` before each commit.

---

### Task 1: The chat id reaches the harness

`OpenTerminal` is chat-scoped, and the harness currently has no idea which chat it serves.

**Files:**
- Modify: `crates/harness/src/lib.rs` (`RunControls`, around `:38-50`)
- Modify: `crates/engine/src/sessions.rs` (the `dispatch` path that builds `RunControls`, around `:214-340`)
- Modify: every other `RunControls` construction site (search `RunControls {`) — tests and `crates/harness/src/mock.rs` included

**Interfaces:**
- Produces: `RunControls.chat_id: String`, populated by the executor at dispatch.
- Consumes: nothing new. `RunRequest` is untouched.

- [x] **Step 1: Add the field and fix every construction site**

Add `pub chat_id: String` to `RunControls` with a doc comment saying it is the chat the run belongs to, host-side only, never serialized. Compile and fix each site the compiler flags.

- [x] **Step 2: Populate it at dispatch**

In `sessions.rs`, fill `chat_id` from the value `dispatch` already holds. Do not thread a new parameter through if the id is already in scope.

- [x] **Step 3: Prove it arrives through dispatch**

The test must cross the executor boundary — a test that builds `RunControls` by hand only proves the struct has a field. Drive `sessions.dispatch` for a known chat with a harness that records the `chat_id` it was handed, and assert it matches. `crates/engine/` is where this belongs, with the mock harness as the recorder.

Run: `cargo test -p comet-harness -p comet-engine`

Expected: PASS.

- [x] **Step 4: Commit**

`feat(harness): carry the chat id on RunControls`

---

### Task 2: The four tools, against a stubbed client

Pure logic first, no MCP and no socket, so the bounded/resumable behavior is pinned by fast tests.

**Files:**
- Create: `crates/mcp/Cargo.toml`, `crates/mcp/src/lib.rs`, `crates/mcp/src/client.rs`, `crates/mcp/src/tools.rs`
- Modify: `Cargo.toml` (workspace `members` + `comet-mcp` in `workspace.dependencies`)

**Interfaces:**
- Produces: `EngineClient` trait and `WorkerTools<C: EngineClient>` with `spawn`, `read`, `wait`, `kill`.
- Consumes: `comet_proto::TerminalEvent`.

- [x] **Step 1: Scaffold the crate**

Mirror an existing small crate's `Cargo.toml` (`crates/update` is a good template). Package name `comet-mcp`, workspace version/edition/license.

- [x] **Step 2: Define the seam**

```rust
#[async_trait]
pub trait EngineClient: Send + Sync {
    async fn open_terminal(&self, chat: &str, device: Option<&str>) -> Result<String, ToolError>;
    async fn write_terminal(&self, id: &str, data: &str, device: Option<&str>) -> Result<(), ToolError>;
    async fn subscribe_terminal(
        &self,
        id: &str,
        after_seq: Option<u64>,
        device: Option<&str>,
    ) -> Result<BoxStream<'static, TerminalEvent>, ToolError>;
    async fn close_terminal(&self, id: &str, device: Option<&str>) -> Result<(), ToolError>;
}
```

`WorkerTools` owns the `worker_id -> device` map (a `Mutex<HashMap<String, Option<String>>>`), read by the other three so the agent never repeats `target_device`. The entry is committed **only after both RPCs of a spawn succeed** — an id mapped by a spawn that then failed is a worker that does not exist, and every later call for it must be a clean not-found.

- [x] **Step 3: Write the failing tests first**

Against a stub `EngineClient` that replays a scripted event list. Cover exactly these:

```
spawn_returns_the_terminal_id
spawn_closes_the_terminal_when_the_write_fails      // no orphan PTY
spawn_that_failed_leaves_no_worker                  // read/wait/kill -> not found
spawn_records_the_device_for_later_calls            // read/wait/kill reuse it
spawn_quotes_a_cwd_containing_spaces
read_returns_within_the_timeout_on_a_silent_worker  // must not hang
read_returns_within_the_timeout_on_a_chatty_worker  // absolute deadline, not per-event
read_resumes_from_after_seq_without_gaps
read_caps_output_and_still_advances_next_seq        // flood must not grow the heap
wait_returns_the_exit_code_and_signal_on_exit
wait_on_an_already_exited_worker_returns_immediately
wait_that_times_out_returns_its_output_and_cursor
wait_resumed_from_next_seq_yields_the_rest_exactly_once   // no dupe, no hole
spawn_past_max_depth_is_refused
kill_closes_and_forgets_the_worker
```

The `spawn_past_max_depth` case takes the depth from `WorkerTools`' own configuration, not from any environment variable.

- [x] **Step 4: Implement until green**

`spawn` = `open_terminal`, then `write_terminal` with `cd <cwd> && <command>\n` when a `cwd` is given and plain `<command>\n` otherwise. Shell-quote the cwd — a worktree under `/Users/First Last/...` is ordinary, and an unquoted `cd` silently runs in the wrong directory. On write failure: `close_terminal`, leave the map untouched, return the original error.

`read`/`wait` = `subscribe_terminal` and drain, tracking the last `seq` seen as `next_seq`. Two bounds are load-bearing and neither is optional:

- **One absolute deadline** for the whole call, computed once before the loop. A per-event timeout never fires against a stream that is always ready, so a flooding worker would pin the call forever.
- **A 1 MiB ceiling on accumulated output**, dropping oldest-first and *still advancing `next_seq` for what it dropped*. The engine caps its own replay buffer at the same figure; a live tail has no such cap, and holding it whole is an unbounded allocation driven by the worker. Callers that need everything redirect to a file.

`wait` completes on `TerminalEvent::Exit`.

Run: `cargo test -p comet-mcp`

Expected: all PASS.

- [x] **Step 5: Commit**

`feat(mcp): worker tool logic over an engine client seam`

---

### Task 3: The MCP server and the `comet mcp-server` subcommand

**Files:**
- Create: `crates/mcp/src/server.rs`
- Modify: `crates/mcp/src/client.rs` (the real `EngineClient` over `RpcClient`)
- Modify: `crates/mcp/Cargo.toml` (`rmcp`, `comet-rpc`, `comet-proto`)
- Modify: `Cargo.toml` (`rmcp` in `workspace.dependencies`)
- Modify: `apps/comet/src/main.rs` (`Command::McpServer`)

**Interfaces:**
- Consumes: `comet_rpc::{connect_ws, RpcClient, methods}`.
- Produces: `comet mcp-server --chat <id> --port <ipc> [--depth <n>]` speaking MCP on stdio.

- [x] **Step 1: Add rmcp**

`rmcp = "3"` in `[workspace.dependencies]`, with the feature set needed for a stdio server. Check the crate's own docs for the exact feature names rather than guessing — get it wrong and it compiles into a client-only build.

- [x] **Step 2: Implement the real client**

`RpcEngineClient` wrapping `RpcClient`: `call_as` for the unary methods, `subscribe` for `SubscribeTerminal`, adding `targetDeviceId` to the params when a device is set. Method name constants come from `comet_rpc::methods` — do not hardcode strings.

- [x] **Step 3: Register the tools**

Four tools named `spawn_worker`, `read_worker`, `wait_worker`, `kill_worker`, with the parameters and returns from the design's table. Descriptions state the bounds plainly: that output is a 1 MiB tail, that `next_seq` is how you resume, that a not-found means the worker aged past its 30-minute window.

Set `instructions` on the server so the agent learns the tools exist without a `CLAUDE.md` entry: what they are for, that `wait_worker` is how you block on a worker, and that `cwd` with a worktree is how you keep two agents off the same checkout.

- [x] **Step 4: Wire the subcommand**

Add `McpServer { chat: String, port: u16, depth: usize }` to `Command` in `apps/comet/src/main.rs`. It connects to `ws://127.0.0.1:<port>`, builds `WorkerTools`, and serves MCP on stdio.

Nothing may be written to stdout except the MCP protocol — stdout is the transport. Logs go to stderr.

- [x] **Step 5: Prove it speaks MCP**

Run the subcommand by hand against a running engine, feed it an `initialize` frame and a `tools/list` on stdin, and confirm the four tools come back.

Run: `cargo build -p comet && cargo test -p comet-mcp`

Expected: PASS, and the handshake answers.

- [x] **Step 6: Commit**

`feat(mcp): serve the worker tools over stdio from comet mcp-server`

---

### Task 4: Hand the server to the agent

**Files:**
- Modify: `crates/harness/src/claude/mod.rs` (`build_command`, around `:148-217`)
- Modify: `crates/harness/src/codex/mod.rs` (the equivalent command builder)
- Test: the existing harness test modules

**Interfaces:**
- Consumes: `RunControls.chat_id` from Task 1, the IPC port, and the comet binary path.
- Produces: `--mcp-config <json>` on the Claude command; `-c mcp_servers.comet.*` on the Codex command.

- [x] **Step 1: Resolve the binary and the port**

Reuse `resolve_comet_bin()` (`crates/tui/src/daemon.rs:242`) rather than writing a second resolver; move it somewhere both callers can reach if that is what it takes. The port is the engine's IPC port, not a constant.

- [x] **Step 2: Build the Claude flag**

Shape of the value:

```
{"mcpServers":{"comet":{"command":"<bin>","args":["mcp-server","--chat","<id>","--port","<port>","--depth","<n>"]}}}
```

**Build it with `serde_json::json!` and `to_string()`, never with `format!`.** The binary path comes from the filesystem and the chat id from the doc; a quote, a backslash or a space anywhere in either one turns a hand-formatted template into malformed JSON that Claude rejects — and the failure surfaces as "no tools", which is the hardest kind to diagnose. `build_command` next door already does exactly this for `--settings` (`claude/mod.rs:206-207`).

Inline JSON, one argument. Do not pass `--strict-mcp-config`.

- [x] **Step 3: Build the Codex flags**

The same server through `-c mcp_servers.comet.command=<bin>` and `-c mcp_servers.comet.args=[...]`.

- [x] **Step 4: Pin it with tests**

Assert both builders emit the flag, that the chat id and port land in the args, and that `--strict-mcp-config` is absent. Include a case with a binary path containing a space and a quote, parsing the emitted argument back with `serde_json` to prove it survives — a string-equality assertion on a hand-built template would pass while the real thing is broken. Assert nothing is emitted when the binary cannot be resolved: a missing binary must degrade to "no worker tools", never fail the run.

Run: `cargo test -p comet-harness`

Expected: PASS.

- [x] **Step 5: Commit**

`feat(harness): hand the comet MCP server to Claude and Codex`

---

### Task 5: End to end

**Files:**
- Create: `crates/mcp/tests/worker_e2e.rs`

- [x] **Step 1: Real engine, real PTY**

Following the `crates/rpc/examples/e2e_driver.rs` pattern: start an engine, create a space and a chat, then drive `WorkerTools` against a real `RpcClient`.

- [x] **Step 2: The cases that matter**

```
sh -c 'echo hi; exit 3'   -> exit_code 3, "hi" in output
sh -c 'sleep 30'          -> wait times out, kill_worker, wait reports the signal
sh -c 'echo a; sleep 1; echo b'  -> wait with a short timeout, resume from next_seq, "a" and "b" each once
```

Run: `cargo test -p comet-mcp`

Expected: PASS.

- [x] **Step 3: Full suite**

Run: `cargo fmt --all && cargo test --workspace && cargo build -p comet`

Expected: PASS. The workspace suite was green before this work; keep it that way.

- [ ] **Step 4: Desktop smoke** — not run here; the orchestrator owns it.

Start the app, open a session, and ask the agent to spawn a real worker (`claude -p 'reply with the word banana'`). Confirm: the `mcp__comet__spawn_worker` chip renders in the transcript, the worker shows up in the Terminal pane, `wait_worker` returns its exit code, and the parent session stays responsive while it runs.

- [x] **Step 5: Commit**

`test(mcp): end-to-end worker lifecycle against a real engine`
