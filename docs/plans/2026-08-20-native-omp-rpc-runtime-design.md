# Native OMP RPC Runtime — Design

## Goal

Add Oh My Pi as a new, independent `OMP` harness in the Comet model picker without removing, replacing, or changing the existing Pi/`pi-acp` harness or any other runtime.

## Decision

Integrate the user-installed `omp` CLI through its native JSONL RPC surface:

```text
omp --mode rpc-ui --auto-approve --no-extensions --cwd <chat cwd>
```

The implementation follows the proven native OMP boundary in `Orchestrator.dev`: correlated RPC commands, bounded newline-delimited frames, live model discovery, model/thinking selection, persisted-session resume, interactive requests, host tools, structured subagent events, and terminal `agent_end` handling.

ACP remains unchanged for Grok, Hermes, Pi, and OpenCode. The existing Pi option continues to use the pinned `pi-acp` adapter and `~/.pi`; OMP uses the separate `omp` binary and `~/.omp` state.

## Architecture

`OmpHarness` implements the existing `Harness` trait. It owns one OMP child per live Comet run and translates the OMP wire into the existing `AgentEvent` stream; the engine continues to own chat persistence, restart/resume bookkeeping, configuration-sensitive runtime reuse, cancellation, and document folding.

The harness is split into focused modules:

- `omp/protocol.rs`: bounded JSONL parsing/serialization, request correlation types, composite `provider/model` identity, and diagnostic redaction.
- `omp/process.rs`: executable discovery, fixed argv, ready handshake, request timeouts, event fan-out, stderr tail, and escalating shutdown.
- `omp/normalize.rs`: OMP text/reasoning/tool/subagent frames to `AgentEvent`.
- `omp/workers_bridge.rs`: registers the existing Comet `workers` controller MCP tool as an OMP RPC host tool and forwards calls to the unchanged controller sidecar.
- `omp/mod.rs`: `Harness` implementation, model/command discovery, session setup/resume, steering, input bridging, attachments, and terminal completion.

No OMP-specific persistence is added to the engine. `AgentEvent::SessionStarted.session_id` carries OMP's persisted `sessionFile`; the engine already stores that opaque native identifier and injects it as `RunRequest.resume` only for the same cwd. A fresh OMP child resumes it through `switch_session`.

## Catalog and selection

`models()` starts a short-lived `--no-session` RPC child and requests `get_state` plus `get_available_models` concurrently. Every UI model id is the lossless composite `<provider>/<modelId>`; setting a model splits only at the first slash and sends both fields through `set_model`.

The model currently reported by `get_state` is moved to the first catalog row. This lets the existing picker default resolution honor OMP's current model without changing the shared picker or the Pi ACP behavior.

Reasoning-capable models expose the existing Comet ladder `Minimal → Low → Medium → High → XHigh → Max`. The selected Comet level is sent through `set_thinking_level`; models that report `reasoning: false` expose no ladder.

## Run lifecycle

For a fresh or resumed run, the harness:

1. Starts `omp --mode rpc-ui` in the request cwd and waits for `ready`.
2. Enables `set_subagent_subscription { level: "events" }`.
3. Resumes `RunRequest.resume` with `switch_session` when present.
4. Registers the `workers` host tool only when `enable_workers_mcp` is true.
5. Applies `set_model` and `set_thinking_level`.
6. Reads `get_state`, emits `SessionStarted`, preflights prompt plus inline images against the aggregate 2 MiB outbound frame limit, and sends `prompt`.
7. Multiplexes OMP frames, Comet steering, interactive answers, cancellation, and child exit.

Follow-ups routed to the live harness use OMP's `steer` command and emit `AgentEvent::Steered`. Configuration changes continue to restart the runtime through the engine's existing `RuntimeConfig` equality gate.

`agent_end` is terminal only when OMP does not mark it non-terminal and no tracked subagent remains active. Non-terminal ends, auto-retry, compaction, and subagent continuation keep the stream alive. A terminal end emits one `Done`; provider errors emit `Error` plus `Done::Errored`; cancellation sends `abort`, escalates child shutdown when necessary, and emits `Done::Interrupted`.

## Workers host tool

OMP RPC supports host-native tools through `set_host_tools`. The new bridge reuses the existing controller MCP sidecar rather than reimplementing Workers actions or linking private Workers internals into the harness:

- Start the current Comet executable with `__workers_mcp__` and the existing controller-only environment markers.
- Run MCP `initialize` and `tools/list` over the existing JSON-RPC client.
- Convert the single returned `workers` definition to the OMP host-tool shape.
- Forward `host_tool_call` as MCP `tools/call` and return its content through `host_tool_result`.

The ACP injection path is untouched.

## Interactive requests

`rpc-ui` requests with `select`, `confirm`, `input`, or `editor` map onto `RunControls::request_input`. Replies are correlated by the OMP request id and sent as `extension_ui_response`. Unsupported UI mutation requests fail closed; `notify` may surface as a bounded `AgentEvent::Error`, while URL opening never happens implicitly.

## UI and platform integration

- Add wire id `omp` and display label `OMP`.
- Show OMP only when the `omp` executable probe succeeds and the device has not disabled it.
- Reuse the existing monochrome `WORKER_OMP` asset in desktop pickers/settings.
- Add the same OMP mark and label to the iOS live-catalog renderer; iOS does not invent a static OMP model catalog.

## Testing

- Pure protocol tests cover frame bounds, malformed JSONL, redaction, response correlation, composite model identity, and current-model ordering.
- A fake OMP RPC fixture covers handshake, model/command discovery, prompt streaming, tools, questions, steering, resume, non-terminal `agent_end`, subagents, cancellation, timeouts, and early exit.
- Workers bridge tests use a fake controller MCP sidecar and prove no sidecar starts when the flag is false.
- Registry/UI tests prove OMP is an additive installed harness with its own icon and that Pi remains present and unchanged.
- Final gates: formatting, focused harness/engine/UI/iOS tests, workspace check, `cargo build -p zeron`, then real-app picker and one minimal OMP turn.

## Done criteria

- The picker shows separate `Pi` and `OMP` rail entries when both CLIs are available.
- OMP lists the live `~/.omp` catalog and initially selects OMP's current composite model.
- A new OMP chat streams text, reasoning, tools, questions, Workers calls, and subagent activity through the existing Comet transcript.
- Steering, interruption, persisted resume, configuration restart, and terminal completion behave deterministically.
- No existing harness behavior or persisted Pi configuration changes.

## Implementation verification — 2026-08-20

Implemented on `feat/native-omp-rpc-runtime` as an additive native RPC harness. The existing Pi rail remains registered through `pi-acp`; no Pi, Claude, Codex, Cursor, Grok, Hermes, or OpenCode source file has a delivery diff.

Validated locally with the installed `omp/17.4.0` runtime:

- OMP RPC fixture suite: all 22 deterministic tests pass; the separate authenticated real-runtime smoke also passes a first turn and a persisted-session follow-up.
- Protocol hardening covers bounded pre-newline reads, outbound attachment budget, response correlation, diagnostic redaction, missing session identity, abandoned subagents, interactive timeout/remote cancellation, Workers call cancellation/limits, and stream-consumer teardown.
- `cargo check --workspace`, `cargo build -p zeron`, the 21-test proto suite, focused engine session/registry tests, the separate Pi-versus-OMP picker assertion, and the Workers controller MCP test pass.
- The complete harness library suite remains at the known baseline: 81 pass and the pre-existing `shell_env::unix::tests::falls_back_when_interactive_attempt_hangs` fails at `shell_env.rs:330`.
- A packaged macOS smoke showed separate enabled `Pi` and `OMP` settings rows with independent marks. No full Xcode installation exists on this machine (`xcode-select` points to CommandLineTools), so the iOS `xcodebuild` gate remains unavailable.
