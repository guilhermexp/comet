# Dual-Mode Sessions — choose Terminal or UI per session

> **Status (2026-07-22):** Directional / not started. This is a decision note +
> plan capturing the product idea ("in the future you choose between terminal
> or UI when you start a session") plus the findings from reading T3 Code's
> source that make it concrete. Nothing here is scheduled. Before starting
> Phase 0, re-verify the protocol claims below against the then-current
> `claude` and `codex` CLIs — both surfaces are young and move fast.

## What prompted this

We studied how **T3 Code** (github.com/pingdotgg/t3code, read 2026-07-22)
builds a chat GUI over Claude Code and Codex. The load-bearing discovery:
neither integration uses the terminal, hooks, or transcript-file scraping.
Each CLI has a **first-party structured surface** designed for exactly this:

- **Claude Code**: headless stream-JSON mode — the `@anthropic-ai/claude-agent-sdk`
  is a thin Node wrapper that spawns the user's own `claude` binary with
  `--input-format stream-json --output-format stream-json` and speaks NDJSON
  over stdio (partial deltas via `--include-partial-messages`, permission
  decisions routed back through an MCP tool). T3's adapter:
  `apps/server/src/provider/Layers/ClaudeAdapter.ts`.
- **Codex**: `codex app-server` — a JSON-RPC-over-stdio child process with a
  published protocol (`openai/codex` → `codex-rs/app-server-protocol`):
  `initialize` → `thread/start` / `thread/resume {threadId}` → `turn/start`,
  streaming `item/agentMessage/delta`, `item/reasoning/textDelta`,
  `item/commandExecution/outputDelta`, `turn/diff/updated`,
  `thread/tokenUsage/updated`, …; approvals arrive as server→client JSON-RPC
  requests that block until answered. T3 generates its client types from the
  upstream schema at a pinned commit rather than using the `codex-acp` ACP
  adapter. T3's adapter: `CodexAdapter.ts` + `CodexSessionRuntime.ts`.
- **ACP** (Agent Client Protocol) is T3's *fallback* for CLIs without a rich
  first-party surface (`cursor-agent acp`, `grok agent stdio`) — not the path
  for Claude/Codex.

All of that is plain child-process stdio protocol work — the kind
`unpeel-host` already hand-rolls three times over (`mcp_host.rs`,
`browser_mcp.rs`, `remote_server.rs`). **No Node runtime required**, so the
no-Node rule holds.

## The product shape

Not "terminal vs UI" as two skins — **two frontends for the same
conversation, chosen per session**:

- **Terminal session** (today's product): provider TUI in a hosted PTY.
  Unchanged.
- **UI session** (new kind): the same provider CLI running headless as a
  structured child process; Unpeel renders the conversation as a native chat
  surface — streamed text/reasoning, tool activity as summary cards,
  approvals as real dialogs.

The new-session flow gains one choice (Terminal / UI), available only for
providers with a structured surface. Everything else about a session —
project, worktree, preset, title, pin, phone visibility — is identical.

### Product-philosophy guardrails (hard)

- A chat UI is **not** IDE chrome — and provider events such as Codex's
  `turn/diff/updated` must not smuggle it in. Normalize generic tool activity
  for transcript continuity, but do not add a privileged file-change list,
  line-count card, diff pane, file tree, or editor. The review surface stays
  screenshots/demos + terminal + transcript for every domain.
- The phone keeps its terminal-first detail view for terminal sessions
  (AGENTS.md: do not replace it with semantic chat). UI sessions are the
  sanctioned home for a phone chat surface, behind the same feature flag.

## Why the architecture already fits

**1. A UI session is still a hosted session.** `unpeel-host` owns the
headless child exactly as it owns a PTY: manifest + pid discipline +
heartbeats + rescan + control socket all transfer. The differences: the
on-disk log is append-only **NDJSON events** (`events.ndjson`) instead of
`output.bin`, and the control socket accepts structured commands
(`user_message`, `interrupt`, `approval_response`) instead of raw bytes.
Replay-from-offset for remote clients works the same way (the remote server's
"replay from disk, subscribe live at the tail" rule applies verbatim).

Unpeel Apps also need a **hybrid** profile in which the same Rust App keeps a
real PTY while publishing `unpeel.ui/1` snapshots over a separate Host side
channel. That profile shares supervision, validation, replay, and controller
routing with structured UI sessions, but it cannot share their stdio pipes:
ANSI and JSON must remain separate. The focused composition is specified in
`docs/plans/unpeel-app-native-rendering.md`.

**2. Mode switching = restart-with-resume, never live migration** — the same
rule as host handoff. We can't retro-attach an app-server to a running TUI,
and don't need to:

- Terminal → UI: kill the TUI (existing identity-guarded path), relaunch
  headless resuming the minted id (`claude --resume <id> --input-format
  stream-json …` / `thread/resume {threadId}`).
- UI → terminal: relaunch the TUI in a PTY via the existing
  `ResumeCommand.resumed(...)` (`claude -r <id>`, `codex resume <id>`).

Minted-at-launch ids (already shipped for claude/gemini/grok; Codex thread id
comes back on `thread/started`) make both directions precise from second zero.

**3. One normalized event schema, two producers.** T3 converged on a small
vocabulary — turn started/completed, item started/updated/completed,
content-delta (assistant text / reasoning / command output), request
opened/resolved, plan updated, token usage — and our `transcripts.rs` block
types are already ~80% of it. Define the normalized event enum once in
`unpeel-core`, then:

- **UI sessions** produce it live from the structured child.
- **Terminal sessions** produce it after-the-fact from provider JSONL
  (today's transcript API, re-expressed).

Phone previews, `read_transcript`, Copy transcript, and the future chat view
all render one schema regardless of session kind.

**4. Whole subsystems become unnecessary in UI mode.** Hooks, output-growth
heuristics, menu-prompt viewport scanning, `send_text`'s bracketed-paste
recipe — all exist because the TUI is opaque. In UI mode: busy/idle is
`turn started/completed`, attention is a first-class approval request, and
Sessions MCP `send_text` maps to a `user_message` command. No hook assets
installed for UI sessions at all.

**5. Approvals become real UI.** Terminal sessions launch with
skip-permissions flags because TUI prompts are unusable at a distance. UI
sessions can afford real approval flow: Codex sends approval requests over
the protocol; Claude routes permission checks through an MCP permission tool
(`--permission-prompt-tool`) — which can point at our own `unpeel` server. The
mode picker in the UI-session flow ("ask / auto-accept edits / full access")
maps to Codex's approval-policy/sandbox config and Claude's `--permission-mode`.

## Plan

Phases are sequential; each lands usable on its own.

- **Phase 0 — protocol verification spike.** From a scratch Rust binary (or
  `unpeel-host` subcommand): drive `claude` stream-JSON (send message, stream
  deltas, resume by id, permission-tool round trip) and `codex app-server`
  (initialize, thread/start, turn/start, approval request/response, resume).
  Output: a short findings doc pinning exact flags, message shapes, and the
  upstream schema commit to generate/hand-write Rust types against. **Kill
  criterion:** if either surface can't do resume + approvals headlessly, that
  provider stays terminal-only and the plan shrinks accordingly.
- **Phase 1 — normalized event schema + UI session kind in the host.** Event
  enum in `unpeel-core`; `session_host.rs` grows a `structured` session kind
  (NDJSON event log, structured control-socket commands, same
  manifest/heartbeat/reap discipline). Re-express `transcripts.rs` output as
  the same enum (additive; existing API stays).
- **Phase 2 — Claude UI sessions end to end (desktop).** Spawn/steer/interrupt
  headless Claude from the app; minimal native chat surface (streamed text,
  reasoning collapsed, tool summary cards, approval dialogs). New-session flow
  gains the Terminal/UI choice for Claude only. Feature-flagged
  (`ExperimentalFeature`).
- **Phase 3 — mode switching.** "Reopen as UI session" / "Reopen in terminal"
  on the session context menu, built on `ResumeCommand` + the Phase 1 kind.
- **Phase 4 — Codex.** JSON-RPC client for app-server (types pinned to the
  Phase 0 schema commit), mapped into the same event enum and chat surface.
- **Phase 5 — phone.** Stream the NDJSON event log to iOS over the existing
  remote/relay transport (replay-from-offset); phone chat view for UI
  sessions, flag-gated per the iOS direction note.
- **Later / maybe — ACP for the long tail** (cursor-agent, grok, opencode) if
  demand shows up. One ACP client would cover several providers, at the cost
  of a third protocol and lossier events. Not before Claude+Codex prove the
  mode.

## Open questions / risks

- **Protocol stability.** Neither surface is a stable public API. Mitigate as
  T3 does: pin the upstream schema (commit/version) per release, keep a
  compatibility probe at session start, and fail into "reopen in terminal".
- **Feature loss in UI mode.** Slash commands, `/compact`, provider-native
  pickers, subagent TUIs — some have protocol equivalents, some don't. The
  escape hatch is always "reopen in terminal" (Phase 3), which must ship
  before or with Phase 2's flag being widened.
- **Resume fidelity across modes.** Resume-by-id is the same mechanism the
  TUIs use for their own `--resume`, so conversation state should carry; the
  risk is per-provider quirks (e.g. mid-turn state). Phase 0 verifies both
  directions explicitly.
- **Two Claude integrations to maintain** (hooks/TUI + headless). Bounded by
  the fact that the terminal side is frozen/shipped; the headless side is new
  code, not a rewrite.
- **Sessions MCP semantics** against UI sessions (`read_screen` has no screen;
  `send_keys` has no keys). Map `read_screen`/`read_output` to a rendered
  event tail, `send_text` to `user_message`, and have `send_keys` return a
  clear "UI session" error.
