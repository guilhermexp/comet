# Functional proof — unify-project-registry

Owner goal: one project registry. What the chat MCP `list_projects` returns is what Settings → Projects shows and what the Workers controller MCP `list_projects` returns; the Sessions tab opens Worker transcripts; the legacy device is gone with its project and chats on the local device.

## Isolation gate (before any process)

- `.tmp/` is git-ignored in this repo (checked 2026-09-29).
- Sandbox: `.tmp/verify/unify-project-registry/<run>/data/` holding copies of `~/.unpeel` (→ `UNPEEL_HOME`) and `~/.zeron` (→ `ZERON_DATA_DIR`), plus `ZERON_WORKTREES_DIR`, `ZERON_WORKTREE_OWNERSHIP_FILE` and `COMET_WORKERS_HOOKS_DIR` under the sandbox. Unix socket paths via a short `/tmp/cur-<run>` profile when needed (see `crates/workers-unpeel/AGENTS.md`).
- The sandbox MUST also set its own `HOME` (the app installs runtime hooks via `dirs::home_dir()` into `~/.claude`, `~/.codex`, `~/.gemini`) and MUST unset inherited `UNPEEL_*` variables (`UNPEEL_SESSION_ID`, `UNPEEL_HOST_CMD`, `UNPEEL_APP_PORT_REGISTRY_FILE`, `UNPEEL_HOOK_TRACE_FILE`), which point into the real `~/.unpeel` when the operator runs inside a Worker. Source the env script with bash, not zsh (`BASH_SOURCE`), or paths fall back to the repo root. Recipe: `.tmp/verify/WT-20260929-cadastro-unico-de-projetos-space-no-comet/run-1/sandbox-env.sh`.
- Registry sync to the edge MUST be off in the sandbox (local profile or no network credentials), so no retirement or Space creation reaches the owner's synced registry.
- Record digest + mtime of the real `~/.unpeel/app-state.json` and `~/.zeron/**/docs.sqlite3` before and after; they must be unchanged.

## Items

| # | Initial state | User action | Observable end result | Failure it must catch |
|---|---|---|---|---|
| 1 | Sandbox copy of real data (4 Spaces; Workers: `denchclaw-crm`, `orchestrator`, six JK `sec-*`) | Start the sandboxed app once | Chat MCP `list_projects`, Workers controller `list_projects` and Settings → Projects list the same ids/names/paths/devices; JK Distribuição is one project with six checkouts; no "Principal not registered"; backup file exists; second start writes no new backup | Any list differing; duplicate `orchestrator`; lost sessions (count Worker sessions per checkout before/after) |
| 2 | After item 1 | Add a new folder from the Workers palette, then from the Orchestrator palette | One new project, visible in both MCP lists and Settings | Two projects for one folder; Workers-only registration |
| 3 | After item 1 | Workers controller `launch_worker` with the JK project id (sandbox preset, trivial command) | Runs in the JK root; existing `sec-*` checkout id still launches in its worktree | Launch into principal fails for unregistered principal; wrong cwd |
| 4 | Project with Worker sessions (live + archived, one launched from a chat) | Settings → Projects → project → Sessions → open a live Worker row; replay an archived row; in Orchestrator sessions, open a chat | Archived-sessions layout with only Worker sessions, each row with its agent icon; each opens the Worker's own session in a side panel beside the list (live terminal+transcript; archived replay without restart); Orchestrator sessions lists the project's chats and opens the transcript read-only; Settings stays open | Orchestrator transcript in the Worker tab; wrong session; restart triggered; navigation leaves Settings |
| 5 | Sandbox registry with legacy device `fdd7f43c…` offline owning `.orchestrator` (77 chats) | Settings → Devices → Retire | Device absent from Settings → Devices and chat MCP `list_devices`; `.orchestrator` on `c4baebda…`; its chats keep ids and open with transcript; a new message runs locally | Chats left on old device; ids changed; local/online device retirable |
| 6 | Negative control | Retire with a Space folder renamed away in the sandbox | Refused naming the folder; nothing changed (registry dump identical) | Partial re-home |

Native surfaces (Settings, Workers terminal) use the project's native QA recipe (screenshots of the sandboxed app); MCP lists via the real stdio servers. Evidence per item under `.tmp/verify/unify-project-registry/<run>/<item>/evidence.md`.

## Real-data run (after acceptance, owner-authorized)

Only after items 1–6 pass: start the real app (migration with backup), confirm item 1 against real data, then the owner retires `fdd7f43c…`.
