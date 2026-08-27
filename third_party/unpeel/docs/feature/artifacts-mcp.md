# Unpeel Artifacts MCP — Plan

Status: **roadmap — deferred** (decided 2026-07-01). Do **not** start this until
Sessions and Browser MCP are solid — see
`docs/roadmap/core-mcps.md` (Artifacts is a "Later Candidate" there, not core).
Written 2026-07-01.

## Risks & sequencing (read this first when picking it up)

- **#1 risk: model adoption, not implementation.** Claude/Codex are trained and
  system-prompted to print output in the terminal and write files into the
  repo (`PLAN.md`, `docs/…`). MCP tool descriptions are weak steering against
  that. The realistic failure mode is building all of this and `create_artifact`
  never being called unless the user explicitly asks. Unpeel's unique lever is
  the hook system (inject steering context at `UserPromptSubmit`).
- **Step 0 is therefore an adoption spike, not the server**: stub
  `create_artifact`/`read_artifact`, inject into Claude, run ~10 realistic
  prompts ("research X and write it up", "plan this trip", "track these
  tasks"), count tool usage with and without hook steering. A day or two. If
  adoption is weak, park the feature without sunk cost.
- **V1 payoff check**: phases 1–2 deliver a markdown pane and a checklist. The
  round-trip checklist (agent writes, user ticks, agent sees the revision bump)
  is the genuinely novel part and may deserve to lead; interactive canvas is
  where the Claude.ai-style magic lives but is phase 3. Decide the emphasis
  from the spike results.
- **Slippery slope**: users who see their document in a panel will ask to edit
  it. The read-only + checkbox-only guardrail is deliberate (AGENTS.md forbids
  editor surfaces); expect sustained pressure against it.
- Markdown rendering in SwiftUI is worse than it sounds (tables, nested lists,
  code blocks are DIY block layout) — budget for a dependency.
- Tool count: 9 is likely 2 too many (`rename_artifact`/`archive_artifact` can
  fold into metadata updates); every tool is context-window tax on every
  granted session.

## What it is

An agent session gets tools to create and edit **artifacts** — documents, lists,
and (later) canvases — that render in a **right sidebar panel** next to the
terminal. The agent does the writing through MCP; the user reads (and lightly
interacts) in the panel. Think Claude.ai artifacts, but provider-agnostic and
living inside Unpeel.

This is squarely on-philosophy: the terminal stays the agent's control surface,
and the panel makes the agent's *output* accessible to people who work visually
— for any domain (a research summary, a trip plan, a task list, a spec), not
code.

**Philosophy guardrails** (from AGENTS.md, non-negotiable):

- Artifacts are *documents the agent produced for you*, not files. The panel
  shows a flat list of titled artifacts — never a file tree or file browser.
- No diff views, no source-code editor pane, no syntax-highlighted code editing.
  A markdown document may contain fenced code blocks (rendered read-only), but
  the panel never becomes an editor.
- The user does not author artifacts in v1; the agent does. User interactivity
  is limited to reading, copying, exporting, and (for lists) toggling checkboxes.

## Artifact types

| Type | Content format | Rendering | Phase |
| --- | --- | --- | --- |
| `document` | Markdown (`content.md`) | Native SwiftUI markdown rendering | 1–2 |
| `list` | Structured JSON items (`list.json`) | Native checklist; user can toggle items | 1–2 |
| `canvas` | Self-contained HTML (`canvas.html`) | Sandboxed `WKWebView` | 3 |

`list` items: `{ "id": "...", "text": "...", "checked": bool }`, ordered array.
Structured (not markdown checkboxes) so the agent gets granular item tools and
the user's checkbox toggles round-trip cleanly back to the agent.

`canvas` is deliberately phase 3: apps/native links no WebKit today, and
rendering agent-authored HTML needs explicit sandboxing decisions (see Phase 3).

## Storage: a project-scoped artifact store

New on-disk root, **outside** `app-sessions` so artifacts survive session GC
(dead sessions are pruned on rescan; a session's `artifacts/` dir dies with it —
fine for browser screenshots, wrong for documents):

```
~/.unpeel/artifacts/<artifact-id>/
  artifact.json        # metadata + revision
  content.md           # type = document
  list.json            # type = list
  canvas.html          # type = canvas (phase 3)
```

`artifact.json`:

```json
{
  "id": "a1b2c3…",
  "type": "document",
  "title": "Competitive research notes",
  "project_id": "…",            // owning project tree (nullable for no-project sessions)
  "created_by_session": "…",     // diagnostic; artifact outlives the session
  "created_at": "…",
  "updated_at": "…",
  "revision": 7,
  "archived": false
}
```

Rules:

- **Scope = project tree**, same reach rule as Sessions MCP project scope
  (`projects_in_same_tree` in `state.rs`): worktree siblings see the parent
  project's artifacts. A session's tools operate on its own project's artifacts
  only. Sessions with no project get session-tree-private artifacts keyed by a
  null project (visible only to that session).
- **Atomic writes**: temp file + rename (same pattern as
  `last-hook-event.json`), so the app's file watcher never reads a half-written
  artifact.
- **Revisions**: `revision` is a monotonically increasing int, bumped on every
  write. Mutating tools accept an optional `expected_revision`; mismatch returns
  a tool error with the current content so the agent re-reads before retrying.
  This is the whole concurrency story for v1 (two sessions editing one artifact
  is possible but rare; last-writer-wins with detection is enough). No version
  history in v1.

## The server: `unpeel-host __artifacts_mcp__`

New module `crates/unpeel-core/src/artifacts_mcp.rs`, cloned structurally from
`browser_mcp.rs` (which itself cloned `mcp_host.rs`):

- `ARTIFACTS_MCP_ARG = "__artifacts_mcp__"`, `SERVER_NAME = "unpeel-artifacts"`.
- Same hand-rolled line-delimited stdio JSON-RPC loop (`run_stdio` →
  `handle_message` → `initialize`/`ping`/`tools/list`/`tools/call`), no SDK.
- Caller identity via `mcp_host::self_session_id()` (`UNPEEL_SESSION_ID` from
  the inherited env, rejecting the unexpanded `${…}` literal).
- One new dispatch block in `crates/unpeel-host/src/main.rs` next to the
  `__browser_mcp__` block.
- Tool errors as `isError: true` content, never JSON-RPC errors.
- Trace lines prefixed `artifacts-mcp` in `~/.unpeel/hooks/trace.log`.
- **Pure disk, no app bridge.** Unlike Sessions MCP start/close, nothing here
  needs app-side state — tools read/write `~/.unpeel/artifacts` directly and the
  app notices via its file watcher. Works with the app closed, like the rest of
  the host tooling. (An optional `/mcp/artifact-focus` bridge nudge is listed
  under Deferred; FSEvents' 0.5 s latency makes it unnecessary for v1.)

### Tools (v1: 9)

| Tool | Args | Behavior |
| --- | --- | --- |
| `artifact_context` | — | Always callable (mirrors `browser_context`): explains access state, lists types, storage semantics. |
| `list_artifacts` | `include_archived?` | Artifacts in the caller's project tree: id, type, title, revision, updated_at. |
| `create_artifact` | `type`, `title`, `content` \| `items` | Creates, returns id + revision. The panel auto-reveals it (app side). |
| `read_artifact` | `id` | Full content + metadata + revision. |
| `edit_artifact` | `id`, `old_string`, `new_string`, `expected_revision?` | Exact-match string replace for documents (token-cheap targeted edits; unique-match required, same contract as editor Edit tools). |
| `replace_artifact` | `id`, `content` \| `items`, `title?`, `expected_revision?` | Full rewrite. |
| `update_list_items` | `id`, ops: `add` (texts, position?) / `set` (item id → text/checked) / `remove` (item ids), `expected_revision?` | Granular list ops so a to-do update doesn't resend the list. |
| `archive_artifact` | `id` | Soft delete; hidden from the panel's default view. No hard-delete tool — the user deletes from the panel. |
| `rename_artifact` | `id`, `title` | Title-only change. |

Server `instructions` steer usage: create an artifact when the user asks for a
document/plan/list or when output is a deliverable the user will want to keep
or watch evolve; prefer `edit_artifact`/`update_list_items` over full rewrites;
don't mirror ordinary chat answers into artifacts.

### Access grants (state.rs)

Exact `BrowserAccess` mirror — one on/off axis, project reach is structural:

- `pub enum ArtifactsAccess { Off, #[default] On }` with lenient
  `from_state_str` (unknown → `Off`) / `as_state_str`.
- `AppState.artifacts_access: HashMap<String, ArtifactsAccess>` —
  deviations-only per-session map.
- `AppState.artifacts_default_access: ArtifactsAccess` — default **On**:
  agents already have full shell, so structured document-writing adds
  convenience, not privilege; the panel adds visibility. Settings ▸ Artifacts ▸
  Off is the master disable.
- Per-call live gate in the server (`load_security`): re-read
  `app-state.json`, `effective_access = grants.get(caller) ?? default`;
  unknown caller or missing manifest → refuse. Revokes and default changes
  apply live, no restart.

### Injection per provider (hook_assets.rs + integrations)

Same three touch points as the Browser MCP, one more of each:

- `SessionHostLaunch.artifacts_mcp_enabled: bool` (`#[serde(default)]`),
  recorded in the manifest as `artifacts_client_registered` (all four manifest
  write sites in `session_host.rs`, including restart).
- **Claude**: `write_claude_artifacts_mcp_config()` →
  `~/.unpeel/mcp/claude-artifacts-mcp.json` (rewritten every launch by
  `install_claude_hooks`), and `claude::startup_command` appends a third
  additive `--mcp-config` when granted (existing skip-if-user-passed-`--mcp-config`
  guard already covers it).
- **Codex**: `codex::configure_host_command` exports
  `UNPEEL_ARTIFACTS_MCP_BIN`; the wrapper script grows a third
  `-c mcp_servers.unpeel-artifacts.*` block (command/args/env with explicit
  `UNPEEL_SESSION_ID`, since Codex spawns MCP servers with a minimal env).
- **Cursor**: skip in v1 (Browser MCP also skips it); revisit with the
  Sessions-style global `~/.cursor/mcp.json` registration if wanted.
- Restart-recommendation token `artifacts-access:on`, emitted only for
  explicit per-session Off→On overrides where the manifest says the client
  wasn't registered — raising the default never mass-nags (same rule as
  `browser-access:on` in `restartRecommendation(for:…)`).
- Restart carries the grant to the new session id; `pruneNativeState` drops the
  old override entry (same as browser/sessions grants).

## Native macOS UI

### Right panel (RootView)

`RootView.appLayout` is a plain `HStack(spacing: 0)` with sidebar + `ContentArea`
(RootView.swift:100–163). Add a **third child after `ContentArea`**, mirroring
the left sidebar's mechanics:

- Own width in `@AppStorage("unpeel.artifacts.width")` (default ~360, min ~280),
  own drag resizer on its leading edge, `store.artifactsPanelCollapsed`
  persisted like `sidebarCollapsed`.
- Slide in/out by animating frame width to 0 with `.clipped()` (the proven
  sidebar pattern — no flash, matches Tommy's perceived-speed bar).
- Toggle: a `TitlebarIconButton` in `titlebarButtons` (⌘⇧B or ⌥⌘A — pick one
  not taken), plus **auto-reveal**: when a rescan detects a new artifact or a
  revision bump in the *selected session's* project tree, open the panel (and
  select that artifact). Never auto-open for background projects; give those a
  subtle badge on the toggle button instead.

### Panel content

- Header: artifact switcher — a compact horizontal list/menu of the current
  project's artifacts (title + type glyph). Flat list, newest-updated first.
  **Not a tree.**
- Body:
  - `document` → native markdown rendering. v1 renderer: `AttributedString`
    with `.full` presentation-intent parsing plus a thin block-layout wrapper
    (headings, paragraphs, lists, blockquotes, fenced code as monospaced
    blocks). No third-party dependency unless this proves too rough — then
    consider vendoring a small markdown-UI package; decide during Phase 2,
    not now.
  - `list` → native SwiftUI checklist. Checkbox toggles are the one write path
    the *app* owns: it edits `list.json` (atomic, revision bump) via a store
    method, and the agent sees the change on its next `read_artifact` /
    revision-mismatch error.
- Item actions (context menu / toolbar): Copy as Markdown, Export… (save
  panel), Rename, Archive, Delete. Delete is user-only (with confirm); the
  agent can only archive.
- Empty state: short copy explaining agents create artifacts here, mirroring
  the launcher/empty-state tone.

### Live updates

Add `~/.unpeel/artifacts` as a third path in `rebuildFileWatcher()`'s FSEvents
stream (UnpeelStore.swift:1493–1528). Reuse the mtime/size `FileStamp` decode
gate so rescans stay cheap. `rescan()` grows an artifact-scan step that
publishes `artifactsByProject: [String?: [ArtifactSummary]]`; the panel and
auto-reveal derive from it. Selected-artifact content is loaded lazily on
selection/stamp change, not on every rescan.

### Grants UI

Straight copy of the browser grant surfaces:

- Store: `artifactsAccessOverrides` / `artifactsDefaultAccess`,
  `setSessionArtifactsAccess` / `setDefaultArtifactsAccess`,
  `persistArtifactsGrants` / `persistArtifactsDefaultAccess` writing
  `artifacts_access` / `artifacts_default_access` into app-state.json via
  `mutateAppStateJSON` (host reads these per call — must live in the file, not
  UserDefaults). Each setter ends in `rescan()`.
- Sidebar right-click: **Artifacts Access** picker right after Browser Access
  in `SessionRowView`'s context menu, gated to Claude + Codex sessions
  (`sessionCanUseArtifacts`, same CLI gate as `sessionCanUseBrowser`).
- Settings: new `SettingsTab.artifacts` + `ArtifactsSettingsPanel` mirroring
  `BrowserSettingsPanel`: header, default-access picker, deviations-only
  "Custom access" list, plus a storage row (artifact count / total size /
  "Reveal in Finder" is fine here — Finder, not an in-app browser) and a
  "Delete all artifacts" maintenance action.

## Phasing

**Phase 1 — server + plumbing (Rust, no UI)**

1. `state.rs`: `ArtifactsAccess`, `artifacts_access`, `artifacts_default_access`.
2. `artifacts_mcp.rs`: store layout, atomic writes, revisions, the 9 tools,
   live gate, project-tree scoping; `main.rs` dispatch.
3. Launch flow: `artifacts_mcp_enabled` → manifest `artifacts_client_registered`;
   Claude config writer + third `--mcp-config`; Codex env + wrapper block.
4. Tests: tool schema count/shape, gate matrix (off/on/default/unknown caller),
   edit semantics (unique match, revision conflict), list ops, project-tree
   visibility, atomic-write survival. Smoke:
   `printf '…' | UNPEEL_SESSION_ID=<id> unpeel-host __artifacts_mcp__`.

Phase 1 is independently shippable: agents can create artifacts and read them
back even before the panel exists (files are plain markdown/JSON on disk).

**Phase 2 — Mac panel + grants UI**

5. Store: grants, watcher path, artifact scan/publish, checkbox write path,
   restart token `artifacts-access:on`.
6. RootView right panel + toggle + auto-reveal; document/list renderers;
   item actions.
7. `ArtifactsSettingsPanel`, sidebar context-menu picker.
8. `swift build`, launch each provider once, verify: create → panel reveals;
   edit → live update; checkbox toggle → agent sees revision bump; Off grant →
   tools refuse live.

**Phase 3 — canvas + remote (separate decision points)**

- `canvas` type in a sandboxed `WKWebView` (first WebKit use in apps/native):
  local-content-only baseline — no network loads, JS allowed but
  non-persistent data store, navigation locked to the artifact file. Ship only
  with these constraints written down.
- iOS/remote: serve artifact content over the remote server alongside the
  existing browser-artifacts route; an artifacts sheet in UnpeelIOS (it already
  uses `WKWebView` and has sheet-panel patterns). Keep the phone terminal-first
  per AGENTS.md — artifacts are a supporting sheet, not a replacement detail
  view.

## Deliberately deferred

- Version history / undo beyond the revision counter.
- User authoring/editing of artifacts in the panel (beyond list checkboxes).
- Cross-project or global artifact reach; sharing/publishing artifacts.
- An `/mcp/artifact-focus` bridge route (FSEvents auto-reveal covers it).
- Cursor injection; other providers follow the usual integration ladder.
- Rich canvas interactivity (forms, embedded apps) — needs the action-policy
  story first, same reasoning as the Browser MCP's deferred policies.

## Files touched (summary)

| Layer | Files |
| --- | --- |
| Server | `crates/unpeel-core/src/artifacts_mcp.rs` (new), `crates/unpeel-host/src/main.rs` |
| State | `crates/unpeel-core/src/state.rs` |
| Launch/injection | `crates/unpeel-core/src/session_host.rs`, `integrations/mod.rs`, `integrations/claude.rs`, `integrations/codex.rs`, `hook_assets.rs` |
| Native store | `UnpeelStore.swift` (grants, watcher, scan, checkbox writes, restart token), `Models.swift` (mirror types) |
| Native UI | `RootView.swift` (panel), new `Views/ArtifactsPanel.swift`, `SettingsView.swift` (tab + panel), `SidebarView.swift` (context menu) |
| Docs | `AGENTS.md` (new "Built-in Artifacts MCP" section when implemented) |
