# Recent Change Corrections Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Correct all nine findings from the local recent-change review while preserving the current Projects, Workers, Chat Transcript Export, and vendored-Unpeel boundaries.

**Architecture:** Keep the existing persistence and UI seams, add narrow pure projections for testability, and make failure state travel through existing result/error channels. Extend the two active OpenSpec changes with regression scenarios before production edits, then archive them only after focused/full gates and native visual QA.

**Tech Stack:** Rust 2024, gpui, serde/serde_json, sha2, libc, Loro-backed app state, OpenSpec.

**Spec:** `docs/plans/2026-08-27-recent-change-corrections-design.md`; `openspec/changes/add-projects-settings-page/specs/projects-settings/spec.md`; `openspec/changes/add-chat-transcript-export/specs/chat-transcript-export/spec.md`

## Global Constraints

- Keep `Chat`, `Session`, `Chat Transcript`, and `Run Journal` terminology exactly as defined in `CONTEXT.md`.
- Keep all commits local; do not push, deploy, publish, tag, or open a PR.
- Work in RED→GREEN→REFACTOR cycles, at most three findings per batch and at most three fix attempts per failing gate.
- Do not add dependencies; `sha2` and `libc` are already available in the owning crates.
- Preserve the worktree after setup failure, but do not launch a Worker from it automatically.
- Never reconstruct unavailable Unpeel provenance as fact; record the known base, current tree id, and historical limitation.
- Treat gpui render behavior as visual tier when the local DOX matrix says no render harness exists.

---

### Task 1: Complete Projects ledger and Settings contracts

**Files:**
- Modify: `openspec/changes/add-projects-settings-page/specs/projects-settings/spec.md`
- Modify: `openspec/changes/add-projects-settings-page/tasks.md`
- Modify: `crates/workers-unpeel/src/project_ledger.rs`
- Modify: `crates/workers-unpeel/src/lib.rs`
- Modify: `crates/ui/src/settings/projects.rs`
- Modify: `crates/workers-unpeel/AGENTS.md`
- Modify: `crates/ui/AGENTS.md`

**Interfaces:**
- Consumes: `WorkersBootstrap`, `WorkersProject::is_group`, `ProjectRow`, `WorktreeConfig`, `ConfigTarget`, `zeron_engine::parse_git_remote`.
- Produces: `live_projects_for_ledger(&WorkersBootstrap) -> Vec<LiveProject>`, duplicate-safe `reconcile`, `config_from_editor`, `config_edit_required`, collision-safe icon paths, and a `RepositoryState::Published { host, owner, repo }` variant.

- [ ] **Step 1: Add correction scenarios to OpenSpec**

Add unchecked correction tasks and scenarios proving:

```markdown
#### Scenario: Organizational groups never become ledger rows
- **WHEN** the Workers bootstrap contains a filesystem project and a group sharing its path
- **THEN** Settings contains one filesystem-project row
- **AND** the persisted ledger contains one entry for that path

#### Scenario: Worktree configuration is editable end to end
- **WHEN** the user changes the target or command groups
- **THEN** the selected supported config file receives the normalized commands

#### Scenario: Project icon files have a complete lifecycle
- **WHEN** an icon is set, reset, or forgotten
- **THEN** the UI renders the set image and app-owned files are deleted on reset or forget
```

- [ ] **Step 2: Write failing ledger tests**

Add pure tests in `project_ledger.rs`:

```rust
#[test]
fn duplicate_live_paths_produce_one_row_and_one_ledger_entry() {
    let outcome = reconcile(
        &[],
        &[
            live("project", "/tmp/repo", Some(10)),
            live("group", "/tmp/repo", Some(20)),
        ],
        30,
    );
    assert_eq!(outcome.rows.len(), 1);
    assert_eq!(outcome.ledger.len(), 1);
}
```

Add a `lib.rs` unit test for `live_projects_for_ledger` with one normal project, one group on the same path, and one worktree on a different path; expect the normal project and worktree only.

- [ ] **Step 3: Run the ledger tests and verify RED**

Run: `cargo test -p zeron-workers-unpeel project_ledger`

Expected: the duplicate-path test fails with two rows/entries, and the bootstrap-projection test fails until groups are filtered.

- [ ] **Step 4: Implement duplicate-safe ledger projection**

Implement `live_projects_for_ledger` beside `projects_with_ledger`, filtering `project.is_group`. In `reconcile`, retain a `HashSet<String>` of normalized live keys and skip later duplicates before consuming or creating entries.

- [ ] **Step 5: Run the ledger tests and verify GREEN**

Run: `cargo test -p zeron-workers-unpeel project_ledger`

Expected: all ledger-focused tests pass.

- [ ] **Step 6: Write failing pure Projects-page tests**

Extend `settings/projects.rs` tests with:

```rust
#[test]
fn repository_state_preserves_the_remote_host() {
    let state = repository_state(
        &git(true, Some("git@git.example.com:team/repo.git")),
        true,
    );
    assert_eq!(state, RepositoryState::Published {
        host: "git.example.com".into(),
        owner: "team".into(),
        repo: "repo".into(),
    });
}

#[test]
fn editor_normalizes_each_command_group_and_detects_real_changes() {
    let config = config_from_editor("bun install\n\n", "brew bundle", "");
    assert_eq!(config.shared, vec!["bun install"]);
    assert_eq!(config.unix, vec!["brew bundle"]);
    assert!(config_edit_required(&WorktreeConfig::default(), ConfigTarget::Comet, &config, ConfigTarget::Comet));
}

#[test]
fn icon_names_are_digest_based_and_component_safe() {
    let one = project_icon_filename("/a-b", "png");
    let two = project_icon_filename("/a/b", "png");
    assert_ne!(one, two);
    assert!(one.len() < 100);
}
```

- [ ] **Step 7: Run Projects tests and verify RED**

Run: `cargo test -p zeron-ui projects`

Expected: tests fail because `Published` has no host and the editor/icon helpers do not exist.

- [ ] **Step 8: Implement repository host and editor state**

Change `RepositoryState::Published` to retain `GitRemote.host`; build the Open URL from that host. Add three `ComposerInput` entities for newline-separated shared/Unix/Windows commands, a selected `ConfigTarget`, and a last-saved config snapshot. Normalize with:

```rust
fn commands_from_editor(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_owned)
        .collect()
}
```

Commit config before project selection changes and on input submission. Offer Cursor only when `cursor_available`; call `worktree_config::save` on the background executor only when config or target changed. Render all three groups with explicit fallback copy and a `$ROOT_WORKTREE_PATH` copy affordance.

- [ ] **Step 9: Implement icon lifecycle and list scrolling**

Use `Sha256::digest(project_path.as_bytes())` for a fixed-length filename and `attachments::format_by_extension` plus bounded file reads to load `Detail.icon_image: Option<Arc<Image>>`. Render the image in the list/detail when available. Add exact app-owned cleanup helpers that refuse paths outside `~/.unpeel/comet-project-icons`, delete the old icon after metadata update, and run the same cleanup during forget. Change the list body from `overflow_hidden` to vertical scrolling.

- [ ] **Step 10: Run focused Projects gates**

Run:

```bash
cargo test -p zeron-workers-unpeel project_ledger
cargo test -p zeron-ui projects
```

Expected: both commands pass without warnings or failures.

- [ ] **Step 11: Update Projects DOX and commit**

Update the nearest AGENTS.md contracts and coverage matrices for group filtering, config editing, icon cleanup, scrolling, and remote-host preservation.

Run: `git diff --check`

Commit:

```bash
git add openspec/changes/add-projects-settings-page crates/workers-unpeel/src/project_ledger.rs crates/workers-unpeel/src/lib.rs crates/workers-unpeel/AGENTS.md crates/ui/src/settings/projects.rs crates/ui/AGENTS.md
git commit -m "fix(projects): complete projects settings contracts"
```

---

### Task 2: Make worktree setup execution and failure propagation reliable

**Files:**
- Modify: `openspec/changes/add-projects-settings-page/specs/projects-settings/spec.md`
- Modify: `openspec/changes/add-projects-settings-page/tasks.md`
- Modify: `crates/workers-unpeel/src/worktree_config.rs`
- Modify: `crates/workers-unpeel/src/lib.rs`
- Modify: `crates/ui/src/workers/model.rs`
- Modify: `crates/workers-unpeel/AGENTS.md`
- Modify: `crates/ui/AGENTS.md`

**Interfaces:**
- Consumes: `SetupOutcome`, `WorkersWorktreeResult`, `WorkersModel::run_action`.
- Produces: bounded concurrent stderr drain, process-group timeout, `SetupOutcome.failed_reason`, and a `WorkersError` path that blocks `create_worktree_and_launch` after setup failure.

- [ ] **Step 1: Add setup-failure regression scenarios to OpenSpec**

Record that setup failure retains the worktree, names command and reason, appears in the UI, and prevents automatic launch. Add a timeout scenario requiring descendant cleanup and bounded stderr handling.

- [ ] **Step 2: Write failing executor tests**

Add tests in `worktree_config.rs`:

```rust
#[test]
fn verbose_stderr_cannot_deadlock_the_runner() {
    let config = WorktreeConfig {
        shared: vec!["head -c 1048576 /dev/zero >&2; exit 7".into()],
        ..Default::default()
    };
    let outcome = run_setup_with_timeout(worktree.path(), main.path(), &config, Duration::from_secs(3));
    assert_eq!(outcome.failed.as_deref(), Some(config.shared[0].as_str()));
    assert!(outcome.failed_reason.as_deref().is_some_and(|reason| !reason.is_empty()));
}
```

Add a Unix-only timeout test that starts a background `sleep`, writes its PID, and asserts `libc::kill(pid, 0)` returns `ESRCH` after the timeout.

- [ ] **Step 3: Run executor tests and verify RED**

Run: `cargo test -p zeron-workers-unpeel worktree_config`

Expected: the verbose test reaches the old timeout/deadlock path or lacks `failed_reason`; the descendant test observes a surviving process.

- [ ] **Step 4: Implement bounded drain and process-group timeout**

Spawn stderr draining immediately in a dedicated thread, cap retained text to the final 64 KiB, and join the reader after exit. On Unix, start the shell in its own process group and signal `SIGTERM`, wait briefly, then `SIGKILL` the negative process-group id before `wait`. Keep the existing `child.kill` fallback outside Unix.

- [ ] **Step 5: Run executor tests and verify GREEN**

Run: `cargo test -p zeron-workers-unpeel worktree_config`

Expected: all setup tests pass within their explicit deadlines.

- [ ] **Step 6: Write failing propagation tests**

Extend the existing worktree wiring tests so `WorkersWorktreeResult` contains both failed command and reason. Add a pure guard test:

```rust
#[test]
fn setup_failure_blocks_launch_but_keeps_the_worktree_result() {
    let existing_worktree = tempfile::tempdir().unwrap();
    let result = WorkersWorktreeResult {
        project_id: "worktree-1".into(),
        path: existing_worktree.path().display().to_string(),
        branch: "change/fix".into(),
        setup_failed_command: Some("bun install".into()),
        setup_failed_reason: Some("exit 1".into()),
        setup_commands_run: 0,
    };
    assert!(ensure_setup_succeeded(&result).is_err());
    assert!(Path::new(&result.path).is_dir());
}
```

- [ ] **Step 7: Run propagation tests and verify RED**

Run: `cargo test -p zeron-workers-unpeel worktree_setup_wiring_tests`

Expected: failure until the reason and launch guard exist.

- [ ] **Step 8: Propagate failure through client and UI**

Add `setup_failed_reason: Option<String>` to `WorkersWorktreeResult`. Call `ensure_setup_succeeded(&worktree)?` before `launch_session`. In the plain-create model callback, retain the worktree selection but set `model.error` to a message naming the command and reason when setup failed.

- [ ] **Step 9: Run focused worktree gates and commit**

Run:

```bash
cargo test -p zeron-workers-unpeel worktree_config
cargo test -p zeron-workers-unpeel worktree_setup_wiring_tests
cargo test -p zeron-ui workers
git diff --check
```

Commit:

```bash
git add openspec/changes/add-projects-settings-page crates/workers-unpeel/src/worktree_config.rs crates/workers-unpeel/src/lib.rs crates/workers-unpeel/AGENTS.md crates/ui/src/workers/model.rs crates/ui/AGENTS.md
git commit -m "fix(workers): make worktree setup failures reliable"
```

---

### Task 3: Sanitize every Chat Transcript Export format

**Files:**
- Modify: `openspec/changes/add-chat-transcript-export/specs/chat-transcript-export/spec.md`
- Modify: `openspec/changes/add-chat-transcript-export/tasks.md`
- Modify: `crates/ui/src/chat_export.rs`
- Modify: `crates/ui/AGENTS.md`

**Interfaces:**
- Consumes: sanitized transcript `SessionMessageEntry` values and `tool_chip_content`.
- Produces: `ExportMessage`, `ExportPart`, and `ExportTool` serializable projections shared by Markdown, Text, and JSON.

- [ ] **Step 1: Add the JSON parity regression to OpenSpec**

Require JSON to omit inline `output`, `diff`, `outputRef`, `diffRef`, reasoning, input, and workflow fields while retaining the same text/tool sequence and artifacts as Markdown/Text.

- [ ] **Step 2: Write the failing export test**

Construct a tool part containing output, refs, diff, and a reasoning sibling, then assert:

```rust
let json = render_json(&doc).unwrap();
assert!(!json.contains("verbose output"));
assert!(!json.contains("outputRef"));
assert!(!json.contains("reasoning"));
assert_eq!(parsed["messages"][0]["parts"].as_array().unwrap().len(), 2);
```

Also assert the projected JSON tool has only its export kind plus the permitted command/path/label field.

- [ ] **Step 3: Run the export test and verify RED**

Run: `cargo test -p zeron-ui chat_export`

Expected: raw `SessionMessageEntry` serialization exposes the forbidden fields.

- [ ] **Step 4: Implement the shared sanitized projection**

Replace `ExportDoc.messages: Vec<SessionMessageEntry>` with:

```rust
#[derive(Clone, Debug, Serialize)]
struct ExportMessage {
    role: ExportRole,
    parts: Vec<ExportPart>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum ExportPart {
    Text { text: String },
    Tool { tool: ExportTool },
}
```

Project `Exec`, Write/Edit/ApplyPatch, Read, and generic tools into an `ExportTool` enum during the same transcript pass that builds artifacts. Make all three renderers consume only these types.

- [ ] **Step 5: Run focused export tests and verify GREEN**

Run: `cargo test -p zeron-ui chat_export`

Expected: all export tests pass and no raw output/reference field appears.

- [ ] **Step 6: Update export DOX and commit**

Run: `git diff --check`

Commit:

```bash
git add openspec/changes/add-chat-transcript-export crates/ui/src/chat_export.rs crates/ui/AGENTS.md
git commit -m "fix(export): sanitize every transcript export format"
```

---

### Task 4: Align vendored Unpeel provenance and DOX

**Files:**
- Modify: `third_party/AGENTS.md`
- Modify: `third_party/unpeel-upstream.toml`
- Modify: `crates/workers-unpeel/AGENTS.md`
- Modify: `AGENTS.md`

**Interfaces:**
- Consumes: previous gitlink base `f27e61a6e4fa5e7180f0cd28c129a3b110a89bbc` and current `HEAD:third_party/unpeel` tree id.
- Produces: truthful vendoring metadata and one non-contradictory DOX chain.

- [ ] **Step 1: Capture the reproducible tree identity**

Run:

```bash
git rev-parse HEAD:third_party/unpeel
git ls-tree 7a0874492e616572d41614e1585e5423bf5368e4 third_party/unpeel
```

Record the returned subtree id and the prior gitlink base without interpreting either as the unavailable local patch.

- [ ] **Step 2: Rewrite the vendoring contract**

Make `third_party/AGENTS.md` state that `unpeel/` is ordinary checked-in vendored source, local compatibility patches are maintained here, `.gitmodules`/gitlink no longer exist, and update review compares subtree identities plus explicit patches. Add `third_party/AGENTS.md` to the root Child DOX Index.

- [ ] **Step 3: Replace stale provenance metadata**

Use explicit fields:

```toml
repository = "https://github.com/unpeel-com/unpeel.git"
base_revision = "f27e61a6e4fa5e7180f0cd28c129a3b110a89bbc"
vendored_tree = "646d0abd6ce8a162f854fc2e6ce93a05a8b855bb"
vendored_from = "working-tree"
local_modifications_count = 16
local_patch_available = false
license = "MIT"
```

Re-run Step 1 immediately before editing metadata and require the captured tree
id to remain `646d0abd6ce8a162f854fc2e6ce93a05a8b855bb`; drift means the provenance
input changed and this task must stop for review.

- [ ] **Step 4: Verify contradictions are gone and commit**

Run:

```bash
rg -n "git submodule|authoritative pin is the gitlink|update the submodule" third_party/AGENTS.md crates/workers-unpeel/AGENTS.md AGENTS.md
git diff --check
```

Expected: no instruction tells an agent to operate Unpeel as a submodule.

Commit:

```bash
git add AGENTS.md third_party/AGENTS.md third_party/unpeel-upstream.toml crates/workers-unpeel/AGENTS.md
git commit -m "docs(vendor): align unpeel provenance and DOX"
```

---

### Task 5: Full validation, native QA, review, and OpenSpec archive

**Files:**
- Modify through archive: `openspec/changes/add-projects-settings-page/**`
- Modify through archive: `openspec/changes/add-chat-transcript-export/**`
- Modify if generated by archive: `openspec/specs/**`

**Interfaces:**
- Consumes: the four implementation commits and both complete active changes.
- Produces: green repo gates, visual evidence, archived OpenSpec changes, and a clean working tree.

- [ ] **Step 1: Run crate-level full gates once**

Run:

```bash
cargo test -p zeron-workers-unpeel
cargo test -p zeron-ui
cargo fmt --all --check
git diff --check
```

Expected: every command exits zero.

- [ ] **Step 2: Run strict OpenSpec validation**

Run the repo's installed OpenSpec CLI strict validation for both change names. If the CLI reports a schema-specific command, use the exact command returned by `openspec instructions apply` rather than inventing flags.

- [ ] **Step 3: Perform native visual QA**

Launch the canonical local app/demo and verify:

1. Projects list scrolls beyond the pane height.
2. A group sharing a parent path does not create a duplicate row.
3. Config target and all command groups persist after navigating away and back.
4. A selected custom icon renders and disappears from both UI and disk on reset/forget.
5. A non-GitHub remote opens its own host.
6. A failing setup leaves the worktree, shows command/reason, and launches no Worker.
7. Download/copy as Markdown, JSON, and Text contain the same projected tools and JSON contains no inline output/ref fields.

- [ ] **Step 4: Perform read-only final review**

Review `HEAD~4..HEAD` for correctness, regression, DOX/OpenSpec compliance, and unrelated changes. Do not edit during the review; if a finding appears, return to a focused RED→GREEN cycle before archiving.

- [ ] **Step 5: Archive both OpenSpec changes**

Use the `openspec-archive-change` skill/CLI for `add-projects-settings-page` and `add-chat-transcript-export`. Confirm the archive updates canonical specs and removes the active-change directories as defined by the workflow.

- [ ] **Step 6: Commit archive closeout**

Run: `git status --short` and stage only OpenSpec archive outputs.

Commit:

```bash
git add openspec
git commit -m "docs(openspec): archive corrected changes"
```

- [ ] **Step 7: Final state check**

Run:

```bash
git status --short --branch
git log --oneline --max-count=6
```

Expected: clean `main`, six new local commits including the design/plan commit, and no push performed.
