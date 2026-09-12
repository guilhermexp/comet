## Context

See proposal.md for motivation. `OmpProcess::start` currently hard-codes `--mode rpc-ui --auto-approve --no-extensions --allow-home`. OMP treats `--no-extensions` as "skip discovery"; `--extension` still works but is not used here. Skill scoping is a separate `--config` overlay and must stay. Worker-maintenance `probe_args` (`--no-extensions --version`) are not Session launch args.

The pre-ready stdout buffer already exists because `--no-extensions` does not silence `extension_ui_request`; with discovery on, those frames remain possible during handshake. RPC already handles select/confirm/input/editor/cancel and ignores other UI extension methods.

## Goals / Non-Goals

**Goals:**

- Default OMP extension discovery on every new Session child.
- Prove the argv change with the existing fake-OMP fixture and a one-shot real-OMP command catalog smoke.
- Keep skill overlay, remaining launch flags, and handshake buffering unchanged.

**Non-Goals:**

- Extension UI parity, new RPC methods, Fusion changes, OMP binary changes.
- Runtime config knob, allowlist, or `--extension` pinning.
- Restarting the running Comet process or mutating existing Chats.
- Changing worker-maintenance probe args.

## Decisions

### D1: Delete the flag; do not replace it

Remove `--no-extensions` from the Session argv. OMP's default discovery is the product behavior the user chose. A toggle or allowlist would be a new feature.

Alternative: pass `--extension` for known paths. Rejected — that is an allowlist, not discovery, and would miss future installs.

### D2: Assert argv on the fake child, not a new builder

The fake OMP fixture already inspects argv (`require-skill-scope`, `reject-system-prompt`). Add a scenario that fails if `--no-extensions` is present and still requires `--mode rpc-ui --auto-approve --allow-home`. Do not extract a launch-arg builder.

### D3: Real catalog smoke is throwaway

A temporary integration that starts the real `OmpProcess` against `/Users/guilhermevarela/.orchestrator` and prints `get_available_commands` `/fh-*` names proves discovery. It is environment-specific, so it is removed after proof. `real_omp_catalog_arrives_whole` remains a second ignored probe.

### D4: Existing Sessions need a host restart

The running Comet already owns Session children started with `--no-extensions`. This change only affects new `OmpProcess::start` calls after the rebuilt binary is launched. Do not quit or restart the live app from this work.

## Risks / Trade-offs

- Ambient extension TypeScript runs inside each new OMP Session. Accepted by the user; unused orca extensions are deleted outside this repo as an operational step, not as product code.
- More `extension_ui_request` frames may arrive before `ready`. The existing pre-ready buffer already absorbs them; this change does not enlarge or remove it.
- Unknown UI extension methods stay ignored. Slash commands and other non-UI extension effects still load. UI widgets are out of scope.
