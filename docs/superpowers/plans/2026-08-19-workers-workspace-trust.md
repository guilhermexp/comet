# Workers Workspace Trust Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make every Comet Worker launch trust its selected workspace before the provider starts, while preserving the preset's approval and sandbox policy and delivering MCP briefings only after launch.

**Architecture:** Add a focused `workspace_trust` module to the Comet Workers client. `LocalWorkersClient::launch_session` resolves the effective project/worktree path and prepares the Claude/Codex workspace-trust store before calling the existing Unpeel mobile launch API. The same launch preparation applies Gemini and Pi native per-session trust controls to the resolved command, avoiding persistent-store races without modifying the pinned submodule. The controller MCP removes `initialText` from the early create request and submits it through the hardened session-input path after the session exists.

**Tech Stack:** Rust, serde/serde_json, TOML-compatible text editing for Codex config, Unpeel local mobile API, Cargo integration tests.

## Global Constraints

- Workspace trust must not add or alter dangerous permission-bypass flags.
- Existing provider configuration and unknown fields must be preserved.
- File updates must be idempotent and use atomic replacement.
- A worktree launch trusts the worktree path, not its parent project path.
- Manual UI launches and controller MCP launches use the same trust preparation.
- Controller briefings use the existing sanitized bracketed-paste submission path.

---

### Task 1: Provider trust-store transformations

**Files:**
- Create: `crates/workers-unpeel/src/workspace_trust.rs`
- Modify: `crates/workers-unpeel/src/lib.rs`
- Test: `crates/workers-unpeel/tests/workspace_trust.rs`

**Interfaces:**
- Produces: `prepare_workspace_trust(command: &str, workspace: &Path) -> Result<(), WorkersError>`.
- Produces deterministic transformations for Claude JSON and Codex TOML plus native per-session Gemini/Pi launch commands.

- [ ] Write failing tests proving Claude adds `hasTrustDialogAccepted` and `hasCompletedProjectOnboarding` while preserving unknown project fields.
- [ ] Run `cargo test -p zeron-workers-unpeel --test workspace_trust` and verify the missing API fails compilation.
- [ ] Implement the minimal Claude JSON merge and atomic write.
- [ ] Write failing tests proving Codex inserts or updates exactly one `[projects."<path>"] trust_level = "trusted"` entry without touching unrelated sections.
- [ ] Implement the minimal Codex section transformation.
- [ ] Write failing launch-preparation tests proving Gemini adds `GEMINI_CLI_TRUST_WORKSPACE=true` once.
- [ ] Implement Gemini's native per-session trust environment in the Comet request.
- [ ] Write failing launch-preparation tests proving Pi adds `--approve` once.
- [ ] Implement Pi's native per-session trust flag in the Comet request.
- [ ] Re-run the focal test and verify all provider transformations pass.

### Task 2: Shared launch preparation

**Files:**
- Modify: `crates/workers-unpeel/src/lib.rs`
- Modify: `crates/workers-unpeel/src/workspace_trust.rs`
- Test: `crates/workers-unpeel/tests/local_actions.rs`
- Test: `crates/workers-unpeel/tests/workspace_trust.rs`

**Interfaces:**
- Consumes: `prepare_workspace_trust` and the bootstrap project/preset records.
- Produces: a prepared launch request whose effective command and path are trusted before `/mobile/sessions` is called.

- [ ] Write failing tests for preset command resolution, direct command resolution, and worktree-path precedence.
- [ ] Run the focal tests and confirm they fail because launch preparation is absent.
- [ ] Resolve project and preset data once before launch, validate the path, prepare trust, and pass the prepared command to the existing request.
- [ ] Re-run the focal tests and verify manual and MCP callers share the behavior.

### Task 3: Deferred controller briefing

**Files:**
- Modify: `crates/workers-unpeel/src/controller_mcp.rs`
- Test: `crates/workers-unpeel/tests/controller_mcp.rs`

**Interfaces:**
- Consumes: `LocalWorkersClient::launch_session`, `deliver_sanitized_text`.
- Produces: `launch_worker` that creates the session without upstream `initialText`, then submits the sanitized briefing after session creation.

- [ ] Write a failing test proving `parse_launch` separates the briefing from the early wire request.
- [ ] Write a failing test proving empty/control-only briefing is rejected before launch.
- [ ] Implement a parsed launch envelope containing the launch request and optional sanitized briefing.
- [ ] Launch first, then deliver the briefing with the existing hardened paste-and-submit helper; return an error that identifies the created session if delivery fails.
- [ ] Run `cargo test -p zeron-workers-unpeel --test controller_mcp` and verify green.

### Task 4: Validation and documentation

**Files:**
- Modify: Workers technical brain/documentation file discovered in the repository.

**Interfaces:**
- Validates all preceding tasks as one product behavior.

- [ ] Run `cargo fmt --all -- --check`.
- [ ] Run `cargo test -p zeron-workers-unpeel`.
- [ ] Run the canonical Workers/UI build gate exposed by the repository.
- [ ] Launch the development app and verify a fresh temporary project opens Claude and Codex without a trust prompt.
- [ ] Verify the controller briefing reaches the agent input after startup and does not alter the preset's permission mode.
- [ ] Review `git diff --check`, update the Comet brain with the trust contract and validation evidence, and report the final working-tree state.
