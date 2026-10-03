## Why

Upstream `zeronsh/zeron` moved from the last synced base `9d3cc8b2` (v0.2.96) to `64ad6f6e` (v0.2.102), then `9e1a1115`: 77 commits in two merges on the same branch. It carries engine fixes the fork lacks (subagent lifecycle identity and restart recovery #676, idle reaper sparing a parked session's subagents #637, run loop spin #604, diff-sync ignoring reads #605), harness work (Pi native RPC #630, OpenCode 2.x #686/#634, Cursor user MCP/plugins #616, Antigravity detection/updates #617, Homebrew CLI updates #661) and many transcript/selection fixes.

## What Changes

- Real merge of `refs/upstream/zeron-main@64ad6f6e`, so the next sync starts from v0.2.102.
- Taken (owner: "pode trazer"): Pi ACP → native RPC (#630), opt-in on-device dictation (`crates/voice`, #591) coexisting with Live Voice, opt-in compact model picker and effort controls (#471, default off), file tree context actions and drag-and-drop moves (#514), right-panel tab close button (#587), tooltips (#628), reduced motion (#642), background positioning/zoom and wallpaper shuffle (#660/#598), side-chat harness choice (#590), faster archive (#602), queue and selection fixes.
- Second merge (`9e1a1115`): Todo checklist panel (#707), inline chat rename (c78bb1c1), per-project new chat button (#737), BMP→PNG attachments (#739), MCP standalone sessions on a chosen device (#706), compact picker fixes (#749/#745), Shift+Backspace (#757), reduced-motion loaders (#754), preview auth callbacks (#763). Skipped: chat Mermaid wiring (#760, fork has its own), composer branch label (#744), Windows drive paths (#727), upstream CI restructuring.
- Agent CLI updates: upstream brew (#661) and Antigravity (#617) update paths unify with the fork's `UpdatePlan::PackageManager` in `harness_updates.rs`.
- CI: upstream `core-tests` (one nextest run) replaces `session-sync-regressions`, kept path-gated like the fork's lanes.
- Not taken: Explorer Subagents/Chats footer changes (#638, `files/sections.rs` stays deleted), update notice changes (`notice.rs` stays deleted), Linux in-app-updater installer layout, Windows CI split, zui/gpui pins (zui stays vendored), version bump (fork stays 0.2.18).

## Capabilities

### New Capabilities

- `upstream-v0-2-102-integration`: fork contracts preserved across the v0.2.102 merge.

### Modified Capabilities

None at the spec level; behavior changes are recorded in the owning `AGENTS.md` files.

## Impact

`crates/{engine,harness,proto,rpc,ui,mobile}`, new `crates/voice`, `Cargo.toml`/`Cargo.lock`, `.github/workflows`, `scripts/ci`.
