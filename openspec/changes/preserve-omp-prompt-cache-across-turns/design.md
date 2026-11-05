# Design

## Context

See proposal.md (Why). Current lifecycle of an OMP chat turn:

1. `OmpHarness::run` (`crates/harness/src/omp/mod.rs`) spawns `omp --mode rpc-ui …`. It then sends `set_subagent_subscription` and `set_ask_dialog`, plus `switch_session` with `provider` and `modelId` when `request.resume` is set. After that it starts the Workers, `zeron` and Computer Use bridges, sends `set_host_tools`, `set_model` and `set_thinking_level`, and finally sends `prompt`.
2. `run_session` loops `while !finished`. `finish_agent_end` returns `true` on `Complete`, which ends the loop, shuts down the bridges and calls `process.shutdown()`. That shutdown writes `session_exit {reason: dispose}` to the JSONL.
3. The engine (`crates/engine/src/sessions.rs`, PERSISTENT SESSION) parks a steerable runtime after `Completed`, with `idle_since`. It already has an OMP special case for update-pending and background drain. Because the OMP stream closes right after `Done`, it falls into the `None if idle_since.is_some()` branch, the per-turn adapter path, and retires the runtime.
4. The next message builds a new `RunRequest` with `resume`, and step 1 repeats.

Per-turn restart alone does not lose the cache: `omp -p -c` restarts every turn and keeps 100% hits. The Comet resume sequence is what changes the request:

- the session is loaded before the host tools exist;
- the tool-roster notice is queued on every turn;
- the system prompt digest changes;
- thinking blocks are then dropped for prefix binding mismatch.

The exact differing bytes have not been captured yet.

## Goals / Non-Goals

**Goals:**
- Turns of a warm OMP chat never respawn the process (spec: Completed OMP turn keeps its runtime warm).
- A resume in a new process reproduces the previous request prefix (spec: Resumed OMP runtime reproduces the cached prefix).

**Non-Goals:**
- Modifying OMP (the `omp` binary or its source). If parity can only be reached inside OMP, that is reported and stays out of this change.
- Changing the idle reaper duration, compaction thresholds, or the orchestrator system prompt text.
- Changing `stops_turn_in_place` or `reconfigures_in_place` for OMP. Stop and configuration changes keep replacing the runtime.
- Changing other harnesses.

## Decisions

### D1. Warm runtime through the engine's existing parking, not a new pool
`run_session` does not end on a `Complete` disposition, including compaction completion:
- It emits `Done` and keeps the process, bridges and event loop alive.
- It leaves the loop only on interrupt, a closed steering mailbox (`steering_open == false` with no turn active), fatal transport or prompt errors, or the event receiver closing.

The engine already routes the next dispatch into a live runtime when `same_runtime` holds, and retires parked runtimes through the reaper, Stop, config change and update.

Alternative rejected: an adapter-owned process cache keyed by session path. It would duplicate the engine's runtime routing, reaper and update-lease logic.

### D2. A message to an idle warm runtime starts a turn with `prompt`
The adapter tracks whether a turn is active:
- **A turn is running:** a `SteerMessage` keeps the current `steer` path, including in-flight steer intents, `wait_for_status` cancellation, and images.
- **No turn is running:** the message is sent as a new `prompt` request through the same code path as the initial prompt: image preflight, `with_attachments` and `prompt_timeout`. Its completion yields exactly one `Done`.

The worker verifies with the fake and with the real binary what OMP does with `steer` while idle. `prompt` is the decided wire command for idle turns unless OMP rejects it.

### D3. Resume parity by capturing first, then fixing in the adapter
First, capture the full provider request bodies of the last request of process N and the first request of process N+1. Capture through any mechanism the installed OMP supports (debug request log, provider base-URL override to a local recorder, or equivalent), with no OMP source change. Diff them, and record the differing fields in this change's `.tmp/verify` evidence.

Then fix in the adapter, preferring in order:
- (a) load the session and register host tools, model and thinking level before OMP builds its first prompt, either via launch arguments or by sending `set_host_tools`, `set_model` and `set_thinking_level` before `switch_session`;
- (b) any other ordering or launch change the diff shows is needed.

Acceptance is the live scenario in the spec, not the diff alone.

### D4. Background work and autonomous wakes stay engine-owned
OMP async results and subagents may wake the session after `Done`. With the process warm, those wakes arrive as autonomous turns on the live stream. The engine's existing autonomous-turn handling (`self_continued_turn`, quiesce) owns them. The adapter must not emit a second `Done` for a turn and must not drop the frames.

## Risks / Trade-offs

- [Warm OMP processes and their bridges hold memory per idle chat] → the existing 30-minute idle reaper bounds it. No new limit is added.
- [`wait_for_status`, interactive questions or steers left half-finished across a turn boundary] → the existing per-turn state (`delivering`, `pending_interactive`, `in_flight_steers`) is reset or carried explicitly at each `Done`, with focused fixture tests for steer-at-boundary.
- [Resume parity cannot be reached from the adapter] → the warm runtime still removes most misses: every turn inside the reaper window. The worker delivers the request diff and stops on D3 with that evidence instead of patching OMP.
- [The live test spends real tokens and uses the user's real OMP credentials] → it is opt-in behind an env var, uses a small Sonnet or Opus prompt, and is run once for acceptance, not in CI.

## Migration Plan

No data migration. Rollback is reverting the adapter change, after which every turn respawns again.
