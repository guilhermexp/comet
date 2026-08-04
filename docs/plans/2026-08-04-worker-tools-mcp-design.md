# Worker Tools over MCP Design

**Date:** 2026-08-04
**Status:** Proposed

## Goal

Give the agent running inside a comet session the ability to delegate work to CLI workers it spawns itself — start one, read its output, wait for it to finish with an exit code, kill it — without blocking its own turn on a shell call that times out.

The engine already does all of this. `OpenTerminal`, `WriteTerminal`, `SubscribeTerminal` and `CloseTerminal` own a PTY that outlives the caller, and `TerminalEvent::Exit { seq, exit_code, signal }` ends the subscription stream on process exit (`crates/proto/src/entities.rs:400`, `crates/engine/src/terminals.rs:69`). What is missing is a path from the agent to those RPCs. The agent's own Bash tool is not that path: it blocks and it dies with the turn, so a twenty-minute worker does not fit in it.

## Shape

A new subcommand of the existing binary, `comet mcp-server`, speaks MCP over stdio and holds a WebSocket client to the engine's IPC port — the same client `crates/rpc/examples/rpc_probe.rs` already uses. The harness passes it to the agent when it builds the command.

```
claude (agent)  ──stdio/MCP──>  comet mcp-server  ──ws://127.0.0.1:27654──>  engine
                                                                               │
                                                                          PTY worker
```

No new binary and no new process supervision: `crates/tui/src/daemon.rs:242` already resolves the comet binary path, and the engine already treats any local process on the IPC port as a first-class client (`crates/engine/src/lib.rs:513-522`).

`rmcp` (`modelcontextprotocol/rust-sdk`, 3.1.0) is the server implementation. It is the only workspace dependency this adds.

## Tools

Four tools, each one a thin wrapper over an RPC that exists today. All four are relay-forwardable (`crates/engine/src/rpc.rs:546-590`), so a worker can run on another device.

| Tool | Parameters | Returns | Backed by |
| --- | --- | --- | --- |
| `spawn_worker` | `command`, `cwd?`, `target_device?` | `{ worker_id }` | `OpenTerminal` + `WriteTerminal` |
| `read_worker` | `worker_id`, `after_seq?`, `timeout_ms?` | `{ output, next_seq, running }` | `SubscribeTerminal`, drain, cancel |
| `wait_worker` | `worker_id`, `after_seq?`, `timeout_ms?` | `{ exit_code, signal, output, next_seq }` or `{ running: true, output, next_seq }` | `SubscribeTerminal` until `Exit` |
| `kill_worker` | `worker_id` | `{ ok }` | `CloseTerminal` |

`worker_id` is the `terminalId` the engine already mints. The tools add no identity of their own, but the server keeps a `worker_id → device` map: `target_device` is chosen once at spawn, and every later call for that worker is forwarded to the same device without the agent having to repeat it. A worker read on the wrong device is a not-found, so this is not optional.

`wait_worker` is the reason this is worth building. `SubscribeTerminal` replays every buffered event with `seq > after_seq` and then tails live, and on `Exit` the engine clears the subscriber senders (`crates/engine/src/terminals.rs:69`), so the stream terminates by itself carrying the exit code — a blocking wait with a real status. A worker that already exited returns immediately: the replay is delivered and the sender drops without ever registering (`terminals.rs:253-256`).

Both readers are bounded and resumable, because neither property is free:

- **Bounded.** A caught-up subscription on a quiet worker yields nothing and would otherwise hang forever. `timeout_ms` is what makes "check on it without committing the turn" real; without it `read_worker` is a trap.
- **Resumable.** `next_seq` is the last sequence included in `output`, and passing it back as `after_seq` continues exactly where the previous call stopped. A `wait_worker` that times out still returns what it consumed plus its cursor, so the output read during a timed-out wait is not lost — otherwise every timeout would punch a hole in the log.

Output is a **tail, not a transcript**: the engine's replay window is capped at 1 MiB and drops oldest-first (`MAX_REPLAY_BYTES`, `terminals.rs:32`). A chatty worker read late has lost its head. Agents that need the whole thing redirect to a file in the command.

`spawn_worker` is two RPCs and can fail between them. If `WriteTerminal` fails after `OpenTerminal` succeeded, the tool calls `CloseTerminal` before returning the error — otherwise the failure leaves a live PTY that the agent never learns the id of, and it occupies a slot until the reaper takes it.

The server ships its own system prompt through `instructions` in the `rmcp` server constructor, so the tools do not need to be announced in the repo's `CLAUDE.md` or in the user's prompt to be discovered. Prior art: `claude-peers-mcp` `server.ts:151-164` — see `brain-source/patterns/ref-claude-peers-mcp-channel-push.md`.

## Ownership and lifetime

A worker is an ordinary engine terminal, so the engine owns it, not the MCP server. It is scoped to the chat, it survives the MCP server and the agent turn that created it — that is the whole point — and it shows up in the Terminal pane, where the user can watch or kill it by hand.

The engine's existing bounds apply unchanged and are the reason this design adds no quota of its own: `MAX_TERMINALS` caps a device at 32 open terminals (`terminals.rs:30`), and an exited session keeps its replay buffer for a 30-minute TTL before the reaper drops it (`EXITED_TTL`, `terminals.rs:33`). Past that window `read_worker` and `wait_worker` return not-found, which is the honest answer: the result is gone.

## Session identity

`RunRequest` (`crates/proto/src/agent.rs:81`) carries no chat id, so the harness cannot name the chat it is running for — and `OpenTerminal` is chat-scoped.

The id travels on `RunControls` (`crates/harness/src/lib.rs:38-50`) rather than on `RunRequest`. `RunControls` is host-side and never serialized; `RunRequest` is persisted verbatim in the CRDT command ledger, so widening it is a doc-schema change for a value the executor already knows. `sessions.dispatch` holds the chat id at the call site and can hand it over directly.

## Injection

`build_command` gains the config for whichever harness is launching.

Claude (`crates/harness/src/claude/mod.rs:148`) takes it inline — `--mcp-config` accepts JSON strings, so no temp file:

```
--mcp-config {"mcpServers":{"comet":{"command":"<comet-bin>","args":["mcp-server","--chat","<chatId>","--port","<ipc>"]}}}
```

Codex takes the same shape through `-c mcp_servers.comet.command=…` and `-c mcp_servers.comet.args=[…]`.

`--strict-mcp-config` is **not** passed. Strict mode would drop the user's own MCP servers from every comet-spawned agent, which is a regression for anyone whose workflow depends on them. The comet server is additive.

## Working directory

`OpenTerminal` derives cwd from the chat and takes no cwd parameter, so `spawn_worker` with a `cwd` writes `cd <path> && <command>` as the first line into the PTY. That keeps the RPC schema untouched. Giving `OpenTerminal` an explicit cwd is the cleaner shape and can come later if the shell prefix proves fragile.

The `cwd` parameter is how a worker gets an isolated worktree: the agent calls the existing `CreateWorktree` and passes the resulting path. Two agents writing the same checkout is the failure this avoids, and it stays the agent's decision rather than a hidden default.

## Recursion

A worker is a plain process in a PTY and inherits no `--mcp-config`, so by default it has no worker tools. That is a default, not a guard: the agent composes the command string, so nothing stops it from writing its own `--mcp-config` and handing the tools to a child.

The enforcement therefore lives where it cannot be bypassed by the command string — in the server, before the spawn. `comet mcp-server` is launched with its own `--depth` (0 for the session the harness starts) and refuses `spawn_worker` past a maximum, returning a tool error the agent can read. A server that hands its config to a child must launch it at `depth + 1`; a child launched with a forged lower depth still hits its own ceiling, because each server checks its own.

The PTY also carries `COMET_WORKER_DEPTH` for observability. That is a signal, not the check — an environment variable is mutable by whatever runs in the shell, so it can never be the thing that stops recursion.

## Scope

No change to the engine's RPC surface, the command ledger, the doc schema, the sync layer, or the UI. Worker output renders in the existing Terminal pane because it is an ordinary engine terminal. Tool calls render as `mcp__comet__spawn_worker` chips through the `ToolCall::Mcp` path that already exists (`crates/proto/src/agent.rs:151`, `crates/ui/src/transcript.rs:2065`).

Out of scope: peer-to-peer messaging between sessions, unsolicited push into a live session via `claude/channel`, worker pools, engine-enforced per-worker timeouts, and any quota beyond the `MAX_TERMINALS` the engine already applies.

## Verification

- Unit tests for the tool layer against a stub RPC client: spawn returns the terminal id; a failed `WriteTerminal` after a successful `OpenTerminal` closes the terminal and surfaces the error; `read_worker` returns within `timeout_ms` on a silent worker; `wait_worker` returns the exit code on `Exit`; `kill_worker` closes.
- Cursor continuity: a `wait_worker` that times out mid-output, resumed with the `next_seq` it returned, yields the remaining bytes exactly once — no duplicate, no hole. This is the property the timeout path exists to preserve.
- Depth: `spawn_worker` at the maximum depth is refused with a tool error, and the refusal does not depend on `COMET_WORKER_DEPTH` being intact.
- Integration test over a real engine with the mock harness, spawning `sh -c 'echo hi; exit 3'` and asserting `exit_code: 3` with `hi` in the output — the `crates/rpc/examples/e2e_driver.rs` pattern.
- A nonzero-exit and a killed-worker case, so failure is distinguishable from success.
- Remote worker: spawn with `target_device`, then read, wait and kill it without repeating the target — proving the `worker_id → device` map holds across calls.
- `cargo test --workspace` and `cargo build -p comet`.
- Desktop smoke: ask the agent to spawn a real `claude -p` worker, confirm the tool chip renders, the worker appears in the Terminal pane, `wait_worker` returns its exit code, and the parent session stays responsive while it runs.
