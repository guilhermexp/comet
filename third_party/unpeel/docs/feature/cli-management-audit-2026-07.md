# CLI Management Audit — Resume, System Context, Status, Start Speed

Date: 2026-07-09. Scope: how well Unpeel "unpeels" each agent CLI — precise
restart/resume, system-context injection, status detection + UI feedback, and
session-start latency — on desktop and mobile. CLI capabilities verified
against locally installed versions (claude 2.1.170, codex 0.143.0, gemini
0.42.0, amp 2026-03-18, opencode 1.17.15, cursor-agent 2026.07.08, grok 0.2.91,
pi 0.57.1; copilot from docs only).

> AGENTS.md's "Resume on Restart" section is **stale**: it describes precise
> resume as Claude-only. The current code (ResumeCommand.swift + hook capture)
> already does precise resume for claude, codex, gemini, amp, opencode,
> cursor-agent, and grok. Update AGENTS.md.

---

## 1. Resume / session-ID matrix

What Unpeel does today vs. what each CLI supports.

| Provider | Unpeel today | ID source today | CLI resume-by-id | CLI pre-assign ID at launch | Untapped ID sources |
|---|---|---|---|---|---|
| claude | precise `--resume <id>`; fallback `--continue` | hook `session_id` | ✓ (`--resume <uuid\|name>`, `--fork-session`, `--from-pr`) | **✓ `--session-id <uuid>`** | `transcript_path` in every hook payload; `~/.claude/projects/<cwd>/<uuid>.jsonl` |
| codex | precise `codex resume <id>`; fallback `resume --last` | notify hook `thread-id` | ✓ (`resume <uuid\|name>`, `fork`) | ✗ | `codex exec --json` `thread.started`; rollout filename `~/.codex/sessions/YYYY/MM/DD/rollout-…-<uuid>.jsonl` |
| gemini | precise `--resume <id>`; fallback `--resume latest` | hook `session_id` | ✓ (uuid, index, or "latest") | **✓ `--session-id <uuid>`** (v0.42) | `--list-sessions`; `~/.gemini/tmp/<hash>/chats/` |
| amp | precise `amp threads continue <id>`; fallback `--last` | plugin `threadID` | ✓ (`threads continue <T-uuid>`) | **✓ `amp threads new`** prints an ID to adopt | `amp threads list` |
| opencode | precise `--session <id>`; fallback `--continue` | plugin root `sessionID` | ✓ (`--session ses_…`, `--fork`) | ✗ | **`opencode session list --format json`** (machine-readable, verified) |
| cursor-agent | precise `--resume <id>`; fallback `--continue` | hook `session_id`/`chatId` (+ `CURSOR_CONVERSATION_ID` env fallback) | ✓ | **✓ `cursor-agent create-chat`** prints a chat ID | hooks officially carry `conversation_id` |
| grok | precise `--resume <id>`; fallback `--continue` | `GROK_SESSION_ID` env forced into hook payload | ✓ (`--fork-session` too) | **✓ `--session-id <uuid>` / `-s`** (must be new) | `grok sessions list`; `~/.grok/sessions/<cwd>/<uuid>/` |
| copilot | hook captures `session_id`; **no restart wiring / no built-in preset** | hook `sessionId` | ✓ (`--resume <id\|prefix\|name>`, exact `--session-id ID`) | ✗ (resume-only) | `~/.copilot/session-state/` + `session-store.db` |
| pi | fallback `--continue` only; **no ID capture at all** (`uses_hook_port: false`) | none | ✓-ish (`--session <path-or-partial-id>`, `--fork`) | ✗, **but `--session-dir <dir>` / `PI_CODING_AGENT_DIR`** relocate storage | `~/.pi/agent/sessions/<cwd>/<ts>_<uuid>.jsonl` — newest file = current session |

### Current pipeline (verified sound)

- Capture: `HookServer.swift:352-373` extracts `session_id`/`chatId`/`thread_id`/
  `conversation_id` variants; persisted per-session in UserDefaults
  (`NativeOverlay.providerSessionIDsKey`, `UnpeelStore.swift:2783-2794`) with a
  manifest `provider_session_id` fallback; read before prune on restart
  (`UnpeelStore.swift:3439-3444`); pruned with the session.
- Rewrite: `ResumeCommand.resumed(...)` is idempotent — strips stale resume
  flags, injects the fresh ID; `fresh()` strips resume for never-written
  sessions.
- Mobile: iOS restart (`SessionOrganizeSheet` → `/mobile` session action →
  `UnpeelStore.applyRemoteSessionAction` → `restartSession`) delegates to the
  exact desktop path, so resume/title/pin/grants are preserved. Good design;
  nothing mobile-specific to fix here.

### Weaknesses

1. **Hook-dependency window.** The ID exists only after the first hook event
   fires. A session that crashes early, has a broken hook install, or was
   launched by an older build restarts on the ambiguous continue-last path —
   which in a shared project root can resume the *wrong* conversation.
2. **Grok/cursor cross-wiring.** Grok's ID comes from the `GROK_SESSION_ID` env
   var relayed through hooks, with special-case suppression logic in the cursor
   hook script — fragile.
3. **Pi is a black box.** No hooks, no ID, no transcript adapter; continue-last
   in a shared root is a coin flip.
4. **Copilot** has full hook + `--resume <id>` support but no built-in
   command/restart integration surface in Unpeel.
5. **No on-disk fallback resolver.** When the hook ID is missing, Unpeel never
   consults the provider's own session storage (which `transcripts.rs` already
   knows how to find for claude/codex/gemini/cursor/grok) to recover the ID.

### Recommendations (resume)

**R1 — Mint the session ID at launch (highest-value change).** For claude,
gemini, and grok, generate a UUID in Unpeel and launch with
`--session-id <uuid>`; for amp run `amp threads new` and adopt the printed ID;
for cursor-agent run `create-chat` and launch with `--resume <id>`. Write it
into the manifest as `provider_session_id` at spawn. Result: precise resume is
guaranteed from second zero, independent of hooks, app uptime, or crash timing
— and the transcript adapter gets an exact file match for previews/read_transcript
immediately. Hook capture stays as confirmation + drift detection.
(Grok bonus: this replaces the env-relay hack — Unpeel *knows* the ID.)

**R2 — On-disk ID recovery fallback.** Before falling back to continue-last on
restart, ask `transcripts.rs` to resolve the session's provider file by cwd +
recency (it already implements this discovery) and extract the ID from the
filename/JSON. Codex rollout filenames, claude/gemini/grok/pi session files,
and `opencode session list --format json` all make this cheap.

**R3 — Fix pi.** Launch pi with a per-Unpeel-session `--session-dir`
(`~/.unpeel/app-sessions/<id>/pi-sessions/` or similar). Then `pi --continue`
is *exact* (only one conversation can live there), restart becomes precise
without any hook, and the JSONL in that dir doubles as a transcript source.

**R4 — Wire copilot restart.** Its hook already posts `sessionId`; add
`copilot` to `ResumeCommand` (`--resume <id>` precise / `--continue` fallback).

**R5 — Verify amp alias.** Current amp help only shows `amp threads continue`;
confirm the generated command is the `threads` form everywhere (the normalizer
already rewrites old `amp continue <id>` — keep that).

**R6 — Transcript adapters for amp + opencode.** Both now hand us stable IDs;
opencode has a documented JSON session store (`~/.local/share/opencode/storage/
session/<projectID>/ses_<id>.json`) — the SQLite note in AGENTS.md is outdated.
Amp threads sync to ampcode.com and local storage is undocumented; the plugin
could stream transcript deltas instead.

---

## 2. System-context injection matrix

Today (commit 5f17117): restart-gated appended context exists for **claude**
(`--append-system-prompt`) and **grok** (`--rules`) only, stored per-session in
UserDefaults, applied on restart via `ProviderSystemContext.appendedCommand`,
with a SHA-based restart-recommendation token. The design is right; coverage is
the gap.

| Provider | Best append mechanism (verified) | Applies | Unpeel today |
|---|---|---|---|
| claude | `--append-system-prompt "…"` (works interactive + print; `--append-system-prompt-file` too) | restart | ✓ |
| grok | `--rules` today; also reads AGENTS.md + full CLAUDE.md-compat files; `grok inspect` verifies discovery | restart | ✓ |
| pi | **`--append-system-prompt "…"`** flag (direct Claude analog); also `APPEND_SYSTEM.md` in `.pi/` or `~/.pi/agent/` | restart | ✗ — trivial add |
| codex | **`-c developer_instructions="…"`** (appends as developer message; does NOT clobber AGENTS.md — avoid `model_instructions_file`, which *replaces* base instructions and breaks GPT-5 validation) | restart | ✗ — wrapper already injects `-c` flags |
| gemini | No flag. `GEMINI_SYSTEM_MD=<path>` env is full-*replace* (dangerous). Right path: a context file + `context.fileName`/`includeDirectories`; **`/memory refresh` re-reads live** | file: live-ish | ✗ |
| copilot | **`COPILOT_CUSTOM_INSTRUCTIONS_DIRS=<dir>`** env → drop an AGENTS.md in an Unpeel-managed dir; docs say instructions re-read **every prompt** — true live injection, no restart | **live** | ✗ |
| opencode | `instructions: [paths/globs/urls]` array in opencode.json — additive merge across config layers, so an Unpeel-managed global layer can add a file without touching user config | restart | ✗ |
| amp | global `~/.config/amp/AGENTS.md` or project AGENTS.md only; no flag | restart | ✗ |
| cursor-agent | project `.cursor/rules/*.mdc` (`alwaysApply: true`) or root AGENTS.md; no flag, no env | restart | ✗ |

### Recommendations (context)

**C1 — Extend `ProviderSystemContext` to codex + pi** (pure flag work, same
restart-gated UX): codex `-c developer_instructions='…'`, pi
`--append-system-prompt '…'`.

**C2 — File-drop tier for copilot/opencode/gemini.** Write the pending context
to `~/.unpeel/context/<session-id>.md` and reference it via
`COPILOT_CUSTOM_INSTRUCTIONS_DIRS` (live, no restart banner needed!),
opencode's `instructions` array, and a gemini context-file include. This makes
the feature near-universal, and for copilot it's the only *live* mid-session
system-context injection any provider supports — a differentiator.

**C3 — Don't use replace-mode mechanisms.** `GEMINI_SYSTEM_MD` and codex
`model_instructions_file` replace the built-in prompt; both are foot-guns.
Note this in code comments so nobody "upgrades" to them.

---

## 3. Status detection + UI feedback

### Per-provider event coverage today

| Provider | Start | PromptSubmit | Stop | Permission/attention | Notes |
|---|---|---|---|---|---|
| claude | ✓ | ✓ | ✓ (+StopFailure) | ✓ | gold standard |
| codex | ✓ | ✗ | ✓ | ✓ | native hooks.json + notify |
| gemini | ✓ | ✗ | ✓ | **✗ silent** | gemini hooks DO have a `Notification` event upstream — unwired |
| copilot | ✓ | ✓ | ✓ | ✓ (preToolUse) | project-local hook file |
| cursor | ✓ | ✗ | ✓ | ✓ (suppressed under grok compat) | |
| grok | ✓ | ✓ | ✓ | ✓ | `allowAttentionClearFromOutput=false` special case |
| opencode | ✓ | ✗ | ✓ | ✓ | root session only; children invisible |
| amp | ✓ | ✗ | ✓ | **✗** | plugin API is WIP |
| pi | ✗ | ✗ | ✗ | ✗ | pure output heuristic (2.5 s window) |

Cross-provider safety nets that already work well: durable
`last-hook-event.json` seed (restores mid-turn busy across app restarts, mtime
anchored), host-side 500 ms menu-prompt viewport scan → `menu_prompt_active` →
attention dot on desktop + `blocked` on iOS, 5-minute hook timeout with
output-growth re-arm.

### Gaps

- **gemini permission prompts are invisible** (no attention, no push). Upstream
  hooks expose `Notification`/`BeforeTool` events — wire them like grok's.
- **amp: no permission events**; menu-scan is the only net.
- **pi: nothing** — and unlike hooks, its 2.5 s output heuristic yields false
  idle on slow token streams and false busy on TUI repaints.
- **opencode child sessions** are filtered out, so subagent work shows idle.
- **No elapsed-time anywhere** ("working 4m12s") — cheap, high glanceability.
- **No dock badge** (count of blocked/attention sessions) on macOS.
- **iOS pushes fire only on done/stop** — a *blocked* session (the state the
  user most needs to act on from a phone) does not push.
- **No token/usage surfacing** (claude/codex transcripts already contain usage
  blocks that `transcripts.rs` parses — it's parsed and dropped today).
- Menu-scan marker lists live in two places (Rust `menu_prompt.rs` + Swift
  `menuPromptActive`) and must stay aligned by hand.

### Recommendations (status)

**S1 — iOS push on `blocked`** (permission + menu-prompt attention), not just
done. This is the single most mobile-valuable status change.

**S2 — Wire gemini's Notification/permission hook events** → PermissionRequest.

**S3 — Elapsed-busy timer** in sidebar row + iOS list (derive from the existing
busy-transition timestamp; no new plumbing).

**S4 — Dock badge** = count of attention sessions (NSApp.dockTile).

**S5 — Usage/context meter (differentiator).** Surface per-session token usage
and context-window fill from the transcript usage blocks already parsed in
`transcripts.rs` — "Claude, 62% context used" in the sidebar tooltip and phone
preview. No other terminal manager shows this.

**S6 — pi**: with R3's per-session dir, watch pi's session JSONL mtime/content
as a busy/idle signal (assistant-message-complete = Stop equivalent), replacing
the raw output heuristic.

---

## 4. Session start latency

Measured path (desktop): write launch.json → spawn `unpeel-host` → host
installs hook assets (~20-50 ms serialized) → writes manifest → opens PTY →
shell + provider start. Two dominating serialized waits:

1. **Manifest poll** in `UnpeelStore` spawn: 40 × 50 ms (up to 2 s) before the
   session row goes live. An FSEvents/dispatch-source watch on the session dir
   (infrastructure exists for rescans) or a tighter 10 ms first-phase poll cuts
   perceived spawn to near-instant.
2. **`.attach-ready` gate**: shell prelude spins up to 2 s (100 × 20 ms)
   waiting for the attach client to sync PTY size (the "narrow banner" fix,
   applied to all providers). Correct, but the polling grain is coarse.

Plus per-launch hook-asset I/O (rewriting MCP JSON/wrappers every spawn) and
provider CLI init itself (0.5–2 s, outside our control).

### Recommendations (speed)

**F1 — Event-driven manifest detection** instead of the 50 ms poll; publish the
session row optimistically at spawn (status `.starting`) so the sidebar and
terminal pane appear at click time, not manifest time. Perceived-speed-first.

**F2 — Tighten the attach-ready handshake**: 5–10 ms sleep grain, and have the
attach client write the ready file at the earliest possible moment (it knows
the grid before the first byte). Alternative: pass the last-displayed grid size
(the Ghostty pane already tracks it) in the launch file so the host can
pre-size the PTY and start the provider immediately, using attach-ready only as
a correction.

**F3 — Pre-install hook assets at app startup** (keyed on host build-id), so
spawn skips the 20–50 ms install unless the binary changed.

**F4 — Pre-warm on intent**: on hover of the "+" / preset menu (hover pre-warm
already exists for surfaces), pre-create the session dir + launch file and
optionally pre-spawn the host paused before exec, so click → keystroke-ready
drops under ~300 ms + provider init.

**F5 — iOS**: same wins apply (phone create delegates to the desktop spawn
path); additionally return the optimistic session row in the `/mobile/sessions`
create response so the phone can navigate into the terminal immediately.

---

## 5. Priority order

1. **R1 mint-ID-at-launch** (claude/gemini/grok flags; amp/cursor mint
   commands) — turns resume from "usually precise" to "always precise", and
   simplifies grok. Small, contained in launch-command construction + manifest.
2. **S1 iOS blocked push + S2 gemini permission events** — mobile's core value
   is "tell me when an agent needs me".
3. **F1 + F2 spawn latency** — the two 2 s ceilings are the whole story.
4. **C1 codex/pi context flags**, then **C2 file-drop tier** (copilot live
   injection is a headline feature).
5. **R3 pi session-dir** (fixes resume *and* enables status + transcripts for
   pi in one move).
6. **R2 on-disk ID fallback + R4 copilot restart + R6 transcript adapters.**
7. **S3/S4/S5 elapsed-time, dock badge, usage meter.**
8. **Docs: refresh AGENTS.md** resume section + provider matrix (opencode
   storage is JSON-file-based now, not "needs a SQLite adapter"; amp resume is
   `threads continue`).
