<!-- Split out of the repo-root AGENTS.md (2026-08-05). The root AGENTS.md holds the map, hard rules, and invariants; this file is the full detail for its topic. -->

## Session Activity State

Busy/idle/attention state lives in `SessionActivity.swift` (native engine) and mirrors the original `session_activity.rs` logic.

Two models exist:

- Hook-driven lifecycle for tools with explicit hook events
- Output-heuristic lifecycle for everything else

The Host's live foreground-runtime observation does not itself switch between
those authority models. A provider launched by a recognized Session command
keeps the existing hook policy. An agent discovered after the user starts it
inside a blank shell remains output/screen-heuristic in this first slice,
because the legacy hook latch is Session-scoped rather than runtime/generation-
scoped and could otherwise apply a previous agent's event to the next agent in
the same shell. The observation still selects live sidebar/icon/tint
presentation, and `menu_prompt_active` still provides attention.

Hook-driven sessions:

- `Start` and `UserPromptSubmit` mark the session busy
- `Stop` marks it idle
- `PermissionRequest` marks attention
- Known hook-capable tools (Claude, Codex, Cline, Cursor Agent, Grok, Kimi,
  Kiro, OpenCode, Amp, Gemini, Copilot) do not use raw output growth to enter busy while waiting for
  the first hook event. This avoids false spinners when a full-screen TUI
  repaints during user scroll or window resize after an app restart.
- The first hook event latches the session as hook-owned; from then on raw
  terminal input never changes its busy/idle state — only hooks and the
  5-minute output-rearmed timeout do.
- **Codex exception — the stop-distrust guard (2026-08-11):** codex fires
  agent-turn-complete `Stop` notifications for *internal sub-turns* of one
  long run, so its long agentic turns used to show idle the whole time. For
  codex only, a hook-idle session whose `output.bin` keeps growing between
  5s and 90s after its latest Stop flips back to busy (then settles through
  the ordinary output-rearmed timeout). The 5s grace skips the turn's
  trailing render burst; the 90s window keeps later user scroll repaints
  from faking busy on a finished session. Implemented identically in
  `SessionActivityEngine` (`distrustStops`) and the TUI `ActivityEngine`
  (`distrust_stops`); the native scan additionally stats hook-idle codex
  sessions, which are otherwise skipped.
- The latch survives app restarts via a durable seed: every provider hook
  script also writes its last lifecycle event to
  `~/.unpeel/app-sessions/<id>/last-hook-event.json` (atomic write; path from
  `UNPEEL_SESSION_DIR`, exported by the host next to `UNPEEL_SESSION_ID`).
  Hook scripts keep firing while no app instance is listening — the port POST
  just fails — so the file records transitions that happen with the app
  closed. On rescan, `UnpeelStore.seedHookActivity` re-seeds an unlatched
  hook-capable session from this file (`LastHookEvent` in
  `SessionActivity.swift`). Seed timestamp: for an **open turn**
  (Start/UserPromptSubmit with no Stop recorded after it) the seed is
  anchored at `max(event mtime, output.bin mtime)` — turns routinely outlive
  the 5-minute hook timeout, and a fresh output.bin means the agent is still
  streaming right now; for everything else the event's own mtime is used, so
  a recorded Stop stays idle no matter how the TUI repaints and a dead
  mid-turn session (both timestamps stale) expires through the ordinary
  5-minute timeout on the first sweep. This restores busy/attention spinners
  for sessions that were mid-turn when the app closed, and correctly stays
  idle when the turn finished while it was closed.

Non-hook sessions:

- Unpeel infers state from output growth (size of `output.bin` over a short window).
- Idle fallback is timer-based.

Agent-drawn select menus (attention, host-side):

- Agent-rendered "pick an option" menus (Claude/Codex numbered prompts) fire
  **no** hook — no `Stop`, no `PermissionRequest` — so hook/output heuristics
  keep a menu-waiting session showing "busy". The **host** closes this gap: it
  already maintains a live parsed viewport per session
  (`TerminalViewportState`), so a 500ms scan thread in `session_host.rs` runs
  the shared detector (`crate::menu_prompt::viewport_has_menu_prompt`, the
  Rust twin of the iOS `menuPromptActive` scan — keep the marker lists aligned)
  over `current_screen_text()` and **edge-writes** `menu_prompt_active` into
  `manifest.json`. Because it lives in the host, it covers **every** session,
  not just ones with a warm Ghostty surface.
- Native reads the flag during `rescan()` and overrides `status → .attention`
  (in `UnpeelStore`), which swaps the busy spinner for the existing yellow
  `AttentionDot` and rolls up to collapsed folders + the iOS `blocked` status
  for free. A generation-bound false → true edge also emits the ordinary
  needs-input notification exactly once; a matching `PermissionRequest` hook
  and visual edge deduplicate whichever one arrives second. The initial app
  scan only seeds state; a session first discovered later can alert even when
  its first sample is already active. False re-arms the next menu. Both the
  badge and visual-edge notification are gated by
  `menuAttentionDetectionEnabled` (Settings ▸ Notifications, default on;
  the `unpeel.native.menuAttentionDetection` UserDefaults overlay).
- The iOS terminal's on-screen menu control bar keeps its own Swift viewport
  scan (it needs the option count + real-time keys); this host flag is the
  desktop badge path, not a replacement for it.

Unread badges integrate with hook events and activity transitions (settles while unobserved → unread).

### Recent ordering and automatic cleanup

Recent ordering and auto-stop/archive consume the same provider-aware
lifecycle timestamp. Hook-capable tools use `last-hook-event.json`; raw
`output.bin` growth is never a fallback for them because attaching, resizing,
and idle TUI repaints can append bytes without real work. Hookless tools prefer
the Host's `screen_changed_at` and use output mtime only for legacy Hosts
without that field. Creation is the floor, and an exited manifest's final
`updated_at` records the exit event; a running manifest's heartbeat-driven
`updated_at` is never activity.

The cleanup clock advances only while the derived status is idle and only when
that canonical lifecycle timestamp advances. Selection, pins, unread results,
attention, active work, and plain shells retain their existing exemptions.

### Herdr projection

When the interactive TUI runs inside Herdr, it projects the already-derived
sidebar state as one aggregate `unpeel` agent. The adapter runs after
`App::rescan`: it must not consume raw hooks or recreate activity semantics.
Any Attention maps to Herdr `blocked`; otherwise Busy/Starting maps to
`working`, and an all-idle or empty live fleet maps to `idle`. See
`docs/agents/herdr.md` for lifecycle, privacy, and environment-containment
rules.
