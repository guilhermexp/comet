# Proposal

## Why

Long OMP chats in Comet lose the Anthropic prompt cache at the start of most user turns. The whole conversation (300k–750k tokens) is rewritten into the cache at write price, so the Opus orchestrator burns the Claude plan several times faster than needed. Measured on 2026-11-05 with the same `omp` 18.16.0 binary:

- 5 consecutive `omp -p -c` processes, Opus `high`, tool calls: the system prompt digest stayed the same, no thinking blocks were dropped, and every turn started with a full cache hit.
- The same script through a Comet OMP chat: the digest changed on turn 2, `thinking_dropped … prefix_binding_mismatch` appeared from turn 2 on (orchestrator chat), and turn 2 read only 13k of a 26k cached prefix.
- Production chat `ac706485a53e`: 27 of 42 user turns missed the cache, costing about US$ 66 of US$ 105. Across the six most expensive sessions of the week, the misses cost about US$ 120 of US$ 252.

The cause is in our adapter, not in OMP:

- `crates/harness/src/omp/mod.rs` ends the OMP process after every `Done` (`finish_agent_end` → `process.shutdown()`), and has done so since `6e0d01e06`. The engine already parks steerable runtimes between turns (`sessions.rs` PERSISTENT SESSION). The OMP stream closes right after `Done`, so the engine retires the runtime.
- Every next turn spawns `omp --mode rpc-ui`, then sends `switch_session`, then `set_host_tools`, then `set_model` and `set_thinking_level`. That request differs from the one the previous process sent:
  - OMP queues a hidden "Tool availability changed" notice on every turn.
  - The system prompt digest changes.
  - Anthropic rejects the signed thinking from earlier processes, and the cached prefix restarts after the tools block.

Claude Code, run through the same engine, keeps 93% of turn-start cache hits.

## What Changes

- An OMP chat runtime stays alive after a cleanly completed turn. The next user message on the same runtime configuration goes to the same process as a new prompt, with no respawn, no `switch_session` and no host-tool re-registration. The existing idle reaper, Stop and configuration-change replacement still retire the process.
- When an OMP runtime has to be resumed in a new process (idle reap, engine restart, configuration change), the first request reproduces the conversation prefix the previous process sent:
  - same system prompt digest;
  - no tool-roster notice;
  - previously accepted thinking blocks are not dropped.
- `crates/engine/AGENTS.md` and `crates/harness/AGENTS.md` describe the OMP runtime lifecycle as it actually behaves. The `engine/AGENTS.md` claim that OMP "estaciona após Done" is currently false.

## Capabilities

### New Capabilities
- `omp-turn-continuity`: How an OMP chat runtime carries one conversation across user turns, both while warm and after a resume, without invalidating the provider prompt cache.

### Modified Capabilities

## Impact

- Code:
  - `crates/harness/src/omp/mod.rs`: `run`, `run_session`, `finish_agent_end`, and the steer and prompt dispatch while idle.
  - Possibly `crates/harness/src/omp/process.rs`, if launch arguments change.
  - `crates/engine/src/sessions.rs`: only if the engine's OMP parking branch needs adjustment.
- Tests:
  - `crates/harness/tests/omp_rpc.rs` and the fake `tests/fixtures/fake-omp-rpc.sh`.
  - Engine integration tests that route a second turn into a parked runtime.
  - A new opt-in live test against the real `omp` binary.
- Docs: `crates/harness/AGENTS.md` and `crates/engine/AGENTS.md`.
- No wire or CRDT change and no new dependency. OMP itself is not modified.
