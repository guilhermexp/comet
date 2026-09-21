## Context

See proposal.md for the observed incident. `controller_mcp.rs::launch_worker` splits `initial_text` from `WorkersLaunchRequest`, starts an empty worker and calls `submit_initial_briefing`. That helper parses the terminal viewport. `is_booting_screen` treats retained `Connecting to MCP servers` text as current startup; the real OMP screen retained that message above a usable prompt. Both actual launches timed out, and a separate send succeeded.

Read-only reference: Orchestrator.dev `src/shared/terminal-provider-commands.ts:315-347` supplies the OMP task as a quoted startup argument; `shouldDeferTerminalBackedAgentInitialPrompt` defers only Codex and Pi. Installed `omp --help` confirms native positional messages and `@file` input. Its `--auto-approve` choice is not copied.

## Goals / Non-Goals

Goals: keep the existing public launch API, move OMP's first input into runtime startup, and retain truthful ownership/task reporting. Provider-specific argument construction belongs at the launch/runtime boundary, not in the outer model prompt.

Non-goals: new retry orchestration, changes to preset configuration, permission bypass, repairs to optional Graft servers, or changes to unrelated providers' startup strategy.

## Decisions

1. Use native OMP startup input. Rejected alternative: special-case the retained warning in viewport matching or increase the timeout. It would keep first-task delivery coupled to presentation. Other runtimes retain guarded interactive delivery.
2. Reuse existing typed launch and runtime command preparation seams; avoid adding a parallel launcher. The implementation must keep task data distinct from trusted command syntax. A private one-shot message file consumed through OMP's supported file input is preferable where direct arguments would expose content in command metadata or interpret leading flags/file markers. Bound content and preserve existing sanitization.
3. Persist parent/task ownership before the first turn can finish. Preserve the current generation/episode completion contract even for fast workers. Do not declare submission merely because a process id exists.
4. Treat initial task as one-shot launch data, not a durable part of the resume command. A retry must not duplicate a submitted task; a restart must not replay the original prompt. Temporary task material stays private and has defined cleanup.
5. Regression seam: `crates/workers-unpeel/tests/worker_initial_briefing.rs`, using the existing isolated subprocess/profile pattern. Test the consumer-observed task and execution count, not source strings. Real controller/OMP smoke complements a deterministic CLI fixture.
6. ACK is not argv calculation. `prepare_native_initial_argv` is idempotent and attaches `@file` only when a body exists. `submit_with_native_reservation` claims `.pending` → `.claimed` immediately before the Host spawn/PTY write, restores pending only on observed pre-submit failure, and never returns pending after a successful submit. Missing body cannot become `.attached`. ACK failure after success keeps the live Host and consumes the reservation. Native startup is PasteAndSubmit only (`native_initial_startup_enabled`). MCP native eligibility uses Host's enabled project-scoped-then-global command via a shared iterator, never `cli_id`. Uncertain native confirmation directs inspection and does not instruct a blind resend.

## Risks / Trade-offs

- Shell/argument interpretation and brief leakage -> use the established quoting/sanitization boundaries and private task transport; exercise literal metacharacters and leading option/file-like text.
- Immediate task completion racing parent registration -> arrange registration/task receipt before execution or prove the existing journal cutoff preserves it.
- A native CLI accepting a message is not proof of model execution -> retain honest delivery semantics and separately observe the literal result in the real smoke.
- Shared checkout and running app -> implement on a new worktree with its own Cargo target; integrate only owned paths and do not stop unrelated processes.

## Migration Plan

No public argument migration. Keep other runtime behavior unchanged. Capture a behavioral RED before code, pass focused and canonical validation, review the launch/security boundary, then exercise the real compiled controller in an isolated profile. Main integrates the scoped change and updates the installed binary atomically using existing installation conventions. Do not restart a live desktop session without first informing the owner; retain the previous binary for rollback.
