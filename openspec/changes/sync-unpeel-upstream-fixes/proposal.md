# Change: Take the upstream Unpeel fixes that apply cleanly

## Why

`unpeel-com/unpeel` is public again (254 commits, last push 2026-09-07). It is
not the history we vendored: the recorded `base_revision`
(`f27e61a6e4fa5e7180f0cd28c129a3b110a89bbc`) does not exist there, the earliest
commit is 2026-08-24, `unpeel-tui` and `unpeel-ui` are gone, and the core is
mid-rewrite (`core_reactor`, `pty_core`, `direct_path*`, `crates/apps`). There
is no common ancestor to merge against, so this is a file-by-file triage, not a
sync.

Three upstream fixes stand on their own against our tree. Everything else was
rejected with a reason recorded below.

## What Changes

- **Menu detection stops firing on a partial repaint.** Claude paints its
  subagent selector progressively, so a scan can land on `↑/↓ to select ·
  Enter to` — after the nav marker, before the `to view` that makes it
  passive. `viewport_has_menu_prompt` read that prefix as an answerable menu
  and raised a brief false attention edge on a Worker that was waiting for
  nothing. The prefix is now passive unless the footer also carries a
  qualifier only a real menu has (`to confirm`, `esc to cancel`, …).

- **Transcript sanitization unwraps `<user_query>` and drops injected
  blocks.** Provider transcripts carry `<system-reminder>`, `<user_info>` and
  `<user_query>` wrappers; the copied transcript showed the XML instead of what
  the user typed, and showed harness injections as if they were user turns.

- **A hook POST no longer goes through the user's proxy.** The hook posts to
  `127.0.0.1`, and curl honours `HTTP_PROXY`/`ALL_PROXY` for loopback too, so
  with a proxy in the environment the lifecycle event went to the proxy and was
  lost — silently, because the POST is `>/dev/null 2>&1`. A lost Start/Stop is
  a Worker stuck in `working`, or hibernation deciding on stale evidence.
  Upstream fixed the shared script; the same line is duplicated in the nine
  per-runtime `lifecycle.sh` assets, and all of them are fixed here — patching
  only the shared one would have left every hook-owned runtime broken.

- **The project projection emits `gitBranch`.** The route the comet uses never
  emitted it, so `WorkersProject::git_branch` was always `None` and every
  non-worktree project reached the Workers titlebar without a branch — a gotcha
  our own docs record. Upstream reads `.git/HEAD` directly (following a
  worktree's `gitdir:` file, and truncating a detached HEAD to a short sha),
  with no subprocess: the projection runs on every bootstrap, so a `git`
  fork per project would not be affordable.

## Capabilities

### New Capabilities

- `workers-upstream-fidelity`: which upstream Unpeel behaviour the vendored
  tree is expected to match, and what the comet deliberately keeps different.

### Modified Capabilities

None.

## Impact

- `third_party/unpeel/crates/unpeel-core/src/menu_prompt.rs` (taken whole — we
  had no local change in it), `transcripts/mod.rs` (sanitization hunk only),
  `controller_host.rs` (`git_head_branch` plus its call and test),
  `hook_assets/scripts.rs` plus the nine `runtimes/*/assets/hooks/lifecycle.sh`
  (the `--noproxy` flag only).
- `third_party/unpeel-upstream.toml` and `crates/workers-unpeel/AGENTS.md`: the
  upstream exists again, and the recorded base revision no longer resolves.

### Rejected, with the reason

- `session_input.rs`, `resume.rs`: upstream **removed** what we added
  (`write_session_input` activity markers, `embedded_conversation_id`,
  `forked`). Taking them would regress hibernation evidence.
- `runtime_observer.rs`: we are ahead — our `parse_procargs2` keeps the argv
  prefix a title-rewriting runtime (Node/libuv) leaves behind; upstream still
  returns `None` there.
- `hook_assets/scripts.rs`: **partially taken.** `--noproxy '*'` on the
  loopback POST is a real fix and came over — see "What Changes". The rest did
  not: the hooks root we deliberately moved (`~/.zeron/workers/hooks`), the
  port-registry `awk` validation (our `app-ports` is one port per line, so the
  current `tr` reads it correctly), the parallel multi-port posting, and the
  `UNPEEL_SESSION_ID` gate. That gate turns out to be safe —
  `integrations/mod.rs:265` sets the variable on every launch — but it is also
  redundant: the script already tests the same variable before posting.
- `relay_crypto` → `relay_wire` rename, `state_bus` pane layouts, `activity_log`
  App alerts, `app_paths::real_unpeel_home`, `remote_attach` refactor: churn or
  features for a surface we do not have.
- `isGroup` on the project projection: upstream still omits the worktree
  clause, so our fix stays a local divergence. Their own
  `is_plain_group` helper and the Swift app both spell it with the clause.

## Impact on behaviour

A Worker that showed a spurious "needs attention" during a Claude subagent
list stops doing so; copied transcripts show the typed text; the Workers
titlebar names the branch of an ordinary project.
