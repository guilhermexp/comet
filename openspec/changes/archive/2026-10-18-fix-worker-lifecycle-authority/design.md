# Design

## Context

See proposal.md. Runtime-specific normalization currently feeds either duplicated shell transports or the shared notifier. Durable seeds lose provider metadata; replay can therefore bypass the HTTP child guard. The parent engine already starts a new turn for a Worker notification when no steerable turn exists.

## Goals / Non-Goals

**Goals:** Apply one lifecycle admission policy at every mutation/recovery boundary; preserve a real runtime's primary interaction transitions and repeated attention episodes. Audit all 16 descriptors and their actual reporting implementations.

**Non-Goals:** Build a plugin framework, integrate remote Box sessions, change parent Chat spinner semantics, sandbox malicious same-user processes, or infer questions from arbitrary terminal text. The prior app-server initialization incident is distinct from Worker lifecycle authority.

## Decisions

1. Managed reporters identify their source using `unpeel_runtime_id` with the catalog's legacy runtime slug. A receiver normalizes this through the runtime catalog and compares it to the Worker's active runtime (or launch runtime when no active observation exists). An explicit runtime mismatch is rejected before role/path checks; declaring `main` cannot override the mismatch. Numeric generations cannot authorize a different provider. Preserve existing legacy handling when no source identity was reported, as required by the existing lifecycle spec; no new reporter may omit source identity.
2. Keep existing provider-specific nested-conversation classification and apply it consistently to preflight, delivery, activity seed loading and journal recovery. Preserve the identity needed for that decision in durable seeds rather than reducing seeds to event names. Mismatched/stale evidence must not change activity, provider bindings, telemetry or parent events.
3. Consolidate provider transports onto the shared notifier where feasible. All delivery modes preflight known listeners before publishing a seed. Explicit rejection suppresses delivery and seed; listener absence preserves accepted-format offline recovery. Keep synchronous ordering for Worker hosts. Do not persist raw prompts/tool inputs merely to retain identity: persist only lifecycle and identity fields needed for recovery, or redact unrelated content before publication.
4. Provider callbacks, not shared-family assumptions, define interaction support. OMP continuation is not completion; ordinary `ask` uses a real tool boundary in addition to approval. Pi uses its actual UI/settle APIs; Amp/OpenCode use their supported question events. Prime uses the current native runtime rather than a nonexistent JS loader. AGY stays hookless.
5. Parent attention keeps one identity per unresolved interaction and retires it on `Start`/`UserPromptSubmit` or a genuine terminal event. Production acknowledgement currently drops the turn-start row while retaining the permission row; recomputing its identity then changes the start sequence to zero and re-notifies. Compact attention journals to the latest event plus its matching generation/episode turn-start so the identity survives acknowledgement. A resolved question followed by another question can notify again; an already answered question cannot wake the parent later. The existing noncompacting test helper missed this, so integration regressions must execute real compaction. Existing engine delivery already covers idle parents and stays unchanged.
6. Verification is isolated: execute actual rendered reporters against loopback capture/admission, use real installed extension hosts without hosted model calls or production configuration mutation, and run focused Worker/vendor suites after integration. Private runtime bootstrap may fetch public metadata/dependencies. Availability gaps for actual provider CLIs are reported, never replaced by a claim of end-to-end proof.
7. Prime Rust 0.10 is an actual launch incompatibility: the verified official release rejects `--extension` before startup. Replace its legacy JS path with native event observation, preserving TUI I/O and an owned private daemon/socket. Explicit `--resume` remains supported; `--continue` now opens a picker and cannot serve as generated automatic resume. The native bridge must use source-defined root/session events, never create a dummy observing agent or attach to an unrelated user daemon.
8. Native lifetime identity is the accepted runtime-launch generation, not the Host's reader/observer reset counter. Capture it with the PTY ownership snapshot and fence cleanup against it. Preserve an explicit saved path after UUID capture only when its bounded session header matches that UUID. The native observer invokes the same canonical managed transport path as the installer, never a reconstructed legacy-home path; isolated smoke keeps these roots distinct.
9. Native shutdown success is a stopped owned listener, not an assumption that the SDK unlinks its Unix socket. After shutdown, remove a remaining owned inode only when the kernel confirms connection refusal; do not unlink a live or unverifiable endpoint. The real SDK smoke exposed this distinction, and a real Unix-listener regression preserves it.
10. Prime Agent Workers get no Computer Use MCP in this release (owner decision, 2026-10-18). The pi-family bridge from origin/main is an `--extension`, which Prime Rust 0.10 rejects, and no other bridge is added. Launch, fresh and resume normalization drop the managed `<unpeel_home>/hooks/mcp-bridge-comet-computer-use.js` argument from persisted Prime commands alongside the legacy lifecycle extension; unmanaged arguments and the exact native resume target are kept. OMP, Pi and the other published Computer Use Workers are unchanged.

## Risks / Trade-offs

- Updating JavaScript on disk does not reload an already-running CLI extension. Live sessions are not restarted by this change; operator restart is needed after a rebuilt app installs assets.
- Runtime stamps prevent accidental cross-provider inheritance, not deliberate forgery by code running under the same user. Same-provider nested identity uses adapter/agent-role evidence, not an invented generic process sandbox.
- Preflight adds bounded loopback requests to asynchronous reporters. This is preferable to publishing rejected completion evidence; unavailable listeners do not lose offline recovery.

## Runtime audit matrix

Reporter code is owned by `third_party/unpeel/runtimes/<package>/assets/hooks`
and its `adapter/setup.rs`; Pi-family assets live under `_shared/pi-family`.
The source-runtime value below is the descriptor's `legacy_slug`, not an
inherited environment guess.

| Package / source runtime | Lifecycle / interaction contract | Proof and limits |
|---|---|---|
| `claude-code` / `claude` | Native turn hooks; filtered `PreToolUse`/`PostToolUse` for `AskUserQuestion`; genuine approval hook retained | Executable shell conformance; actual question tool must not depend on approval |
| `codex` / `codex` | Native lifecycle/approval hooks; legacy notify normalizer stamps source | TUI `request_user_input` is not an external notify callback; full attention is not claimed |
| `cline` / `cline` | `PreToolUse`/`PostToolUse` wrap actual `ask_question`; `TaskComplete` ends a completed native run | Pinned SDK source proves UI ordering; nonempty `parent_agent_id` excludes children before publication |
| `cursor-agent` | Native hooks; outward `continue: true` preserved | An auto-continuing tool hook is not proof of blocked input |
| `gemini` | Native agent/tool hooks; `Notification: ToolPermission` | No invented `AskUser` notification types; question hook ordering has an upstream limitation |
| `github-copilot` / `copilot` | `agentStop` ends the main turn; filtered actual `notification` events identify permission/elicitation prompts | Ordinary `preToolUse` must not mark every running tool blocked; nested-agent payloads are excluded |
| `grok` | Provider-native hooks including filtered question tool attention | Source identity and actual transcript metadata preserved; metadata-only start cannot overwrite a terminal seed |
| `kimi` | Provider-native permission/question hooks and transcript resolution | Keep provider output/config compatibility; shared environment does not authorize a foreign runtime |
| `kiro` / `kiro-cli` | Native v2/v3 lifecycle normalization and transcript resolution | No full attention promise from the current adapter |
| `muse-code` / `muse` | Native lifecycle/approval hooks and recovered managed environment | Preserve POSIX hook output; approval alone does not prove every question is observable |
| `amp` | Plugin `agent.start`/`agent.end` and filtered `tool.call`/`tool.result` for built-in `ask_user_choice` | Genuine ordinary-question support; no full approval/custom-dialog guarantee |
| `opencode` | Root-only busy/idle; permission/question asked/replied/rejected in supported v1/v2 APIs | Native 1.19.3 question/reply flow observed through `/question`; v2 schemas also exposed, not assumed to be the only emitted version |
| `omp` | Real `ask` tool call/result and approval requested/resolved; continuation-aware `agent_end` | Native 18.12.0 `ask` flow observed with main-agent identity |
| `pi` | `agent_settled`; actual blocking extension UI prompt start/end | Native 1.2.0 `ctx.ui.select` observed; no OMP-only callbacks subscribed |
| `prime-agent` | Current Rust 0.10 native start/end stream, replacing unsupported TS-extension injection | Official binary rejects `--extension`; explicit resume retained, `--continue` changed to an interactive picker |
| `agy` | Hookless output-derived fallback | Remains hookless; no synthetic completion or attention |

Primary API references:
- [OMP extension event types](https://github.com/can1357/oh-my-pi/blob/7b92f745f68cf1936a5a2f35e62a5e9a2fd2f03e/packages/coding-agent/src/extensibility/extensions/types.ts) and [continuation contract](https://github.com/can1357/oh-my-pi/blob/7b92f745f68cf1936a5a2f35e62a5e9a2fd2f03e/packages/coding-agent/src/extensibility/shared-events.ts).
- [Pi prompt/settle API](https://github.com/badlogic/pi-mono/blob/main/packages/coding-agent/src/core/extensions/types.ts).
- [Copilot official hook reference](https://docs.github.com/en/copilot/reference/hooks-reference).
- [Codex question-hook gap](https://github.com/openai/codex/issues/19328) and [legacy notify limitation](https://github.com/openai/codex/issues/19921).
- [Gemini notification enum](https://github.com/google-gemini/gemini-cli/blob/main/packages/core/src/hooks/types.ts) and [question hook ordering](https://github.com/google-gemini/gemini-cli/issues/20605).
- [Cline question tool](https://github.com/cline/cline/blob/a42df8c8d5fd9d9f9a9073589baffe32ea9bb2f0/sdk/packages/core/src/extensions/tools/definitions.ts#L814-L835) and [awaited pre-tool boundary](https://github.com/cline/cline/blob/a42df8c8d5fd9d9f9a9073589baffe32ea9bb2f0/sdk/packages/agents/src/agent-runtime.ts#L2440-L2492).
- [Amp built-in question tool](https://ampcode.com/news/neo) and [plugin event contract](https://ampcode.com/docs/plugin-api).
- [Prime Rust 0.10 parser](https://github.com/PrimeIntellect-ai/prime-agent/blob/v0.10.0/crates/pa-cli/src/args.rs) and [interactive resume contract](https://github.com/PrimeIntellect-ai/prime-agent/blob/v0.10.0/crates/pa-cli/src/interactive_mode.rs#L594-L635).

## Native verification already observed

- OMP 18.12.0: isolated `rpc-ui`, actual built-in `ask`, user response `Proceed`,
  and terminal result. Captured `Start → PermissionRequest(ask) →
  UserPromptSubmit → Stop`; exactly two scripted loopback model requests.
- Pi 1.2.0: isolated offline RPC extension command opened actual `ctx.ui.select`.
  Answer `Proceed` produced `PermissionRequest(select) → UserPromptSubmit`,
  with no model turn and no premature `Stop`.
- OpenCode 1.19.3: isolated headless native server loaded the rendered plugin.
  Its actual `question` tool awaited a `/question` reply. `Proceed` resumed
  the turn and produced `Start → PermissionRequest(question) →
  UserPromptSubmit → Stop`, with two scripted loopback model requests.
  Public runtime metadata/dependency bootstrap was allowed after a
  network-isolated bootstrap timed out; no external model was used.
- These probes used private HOME/provider/XDG directories. Their hosts,
  loopback services and temporary directories were removed. Production
  Workers, credentials and app processes were not restarted or modified.
- Prime Rust 0.10.0: three isolated runs used the actual app ingress/polling,
  production Host/journal and native protocol 7. Managed history and an
  explicitly selected external file both retained the exact provider UUID
  across two launch generations in one PTY, without replaying the task.
  Each run made two loopback model requests. A held response could not
  complete early; interrupting the second turn cleaned the owned daemon
  without publishing `Stop`. Scope return and Host teardown both released
  their private endpoints, including SDK shutdown that leaves a socket inode.
  The hook root deliberately differed from the state root, and provider
  binding was read from the canonical marker. Model input used the native
  API; a real Host input activated the existing non-draft resume contract.
  This is not a full native-TUI keyboard verification: that private keyboard
  harness also failed without Comet, so it was not used as lifecycle proof.

## Verification coverage and review limits

- All ten rendered shell reporters passed 50 online/offline,
  synchronous/asynchronous admission cases. Cline/Copilot/Grok passed 12
  targeted interaction transitions and three offline child-suppression cases.
- Real extension/provider execution above covers OMP, Pi, OpenCode and Prime;
  the other runtime mappings are supported by executable reporter/plugin
  regressions and the cited APIs, not an invented native-UI end-to-end claim.
- Two paired review snapshots found transport, fixture, syntax and native
  ownership/resume issues that were addressed. The reported offline-seed race
  was refuted by foreground preflight/seed ordering; OpenCode replies without
  required request IDs are intentionally not accepted as matching replies.
  Both secret scans were clean. The OpenQodex review reported incomplete
  coverage, so no independent approval is claimed.
- A real offline notifier execution reproduced losing supported camel-case
  provider/transcript identity during projection. Canonical normalization was
  corrected; 17 actual notifier executions then preserved the required recovery
  identity without persisting prompts. The permanent offline boundary case
  covers this normalized identity rather than preserving arbitrary raw aliases.
  Obsolete source-string and raw-forwarding pins were removed, not repinned.

## Focused regression evidence

| Suite | Observed result |
|---|---|
| Worker event journal and recovery | 12 passed |
| Parent attention and notification acknowledgement | 42 passed |
| Worker session actions | 4 passed |
| Worker initial briefing | 13 passed |
| Managed hook migration | 6 passed |
| Vendored Unpeel Core, including executable reporters and native ownership | 719 passed; 2 live-auth tests ignored |
| Vendored activity/TUI filter | 26 passed |
| Engine Worker notice without a live mailbox starts a durable parent turn | 1 passed |
| Vendored Core test-target compilation after unused-import cleanup | Passed with no compiler warnings |
