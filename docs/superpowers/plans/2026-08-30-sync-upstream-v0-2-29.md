# Upstream v0.2.29 Port Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Port compatible upstream behavior through `upstream/main@b3fa5187` into the private fork without losing fork-specific runtime, UI, sync, publication, or provider contracts.

**Architecture:** Record upstream ancestry in a retained isolated worktree, preserve private files by default, then reconcile the 60 overlapping paths capability-by-capability. For each absent behavior, import or adapt the upstream regression test first, observe RED, and port only the implementation required for GREEN; equivalent fork behavior stays in place.

**Tech Stack:** Rust 2024 workspace, GPUI, Loro CRDT, Cloudflare Workers/TypeScript, OpenSpec, macOS/iOS packaging.

**Spec:** `openspec/changes/sync-upstream-v0-2-29/`

## Global Constraints

- Upstream target is exactly `b3fa51872f70c8f973c241b659cf0c166766f4f5`; prior behavioral baseline is `v0.2.18@04b08ea2712e98ec0c4b7e302dc4985a79223b16`.
- Never push to `upstream`; do not push the integration branch, tag `v*`, deploy edge, or invoke TestFlight.
- Preserve fork branding, Managed Provider Usage, local-first mode, Workers MCP/runtime, Chat Transcript Export, developer inspector, updater policy, and all active OpenSpec changes.
- Keep GPL Zed crates excluded and preserve MIT/third-party attribution.
- Use canonical `Chat` for durable conversation and `Session` only for device-local execution state.
- Run `cargo fmt --all` before merging upstream; use focused gates during work and the complete gate once at the end.

---

### Task 1: Planning and isolated merge

**Files:**
- Create: `fork_changelog.md`
- Create: `fork_sync_report.md`
- Modify: merge metadata only before capability reconciliation
- Test: repository status, ancestry, and report inspection

**Interfaces:**
- Consumes: clean `main@6519eb68`, `upstream/main@b3fa5187`
- Produces: retained `chore/upstream-sync-v0.2.29` worktree with an auditable merge commit

- [ ] **Step 1: Validate and commit planning artifacts**

Run:

```bash
openspec validate sync-upstream-v0-2-29 --strict
cargo fmt --all
git diff --check
git add openspec/changes/sync-upstream-v0-2-29 docs/superpowers/plans/2026-08-30-sync-upstream-v0-2-29.md
git commit -m "docs: plan upstream v0.2.29 port"
```

- [ ] **Step 2: Seed the prior upstream marker**

Create `fork_changelog.md` with `Last synced upstream commit: 04b08ea2712e98ec0c4b7e302dc4985a79223b16` before invoking the sync wrapper so novelty is measured from the fork's real behavioral baseline.

- [ ] **Step 3: Run the isolated merge without publication**

Run:

```bash
bash ~/.codex/skills/fork-safe-upstream-sync/scripts/sync_in_worktree.sh \
  --sync-branch chore/upstream-sync-v0.2.29 \
  --worktree-path /Users/guilhermevarela/Documents/Projetos/SelfHosting/comet-upstream-v0.2.29 \
  -- \
  --upstream upstream \
  --base-branch main \
  --origin origin \
  --no-push-origin \
  --changelog fork_changelog.md \
  --report fork_sync_report.md \
  --test-command "cargo test" \
  --final-build-command "cargo build -p zeron" \
  --protected-paths "dist/,third_party/,openspec/,*.db,*.sqlite"
```

- [ ] **Step 4: Review merge output**

Run `git status`, `git show --stat --summary HEAD`, inspect `fork_sync_report.md`, and compare the 60 overlapping files against both parents. Reject upstream updater, release, deploy, TestFlight, branding, or private-capability regressions before code work.

### Task 2: Registry and sync correctness

**Files:**
- Modify/Test: `crates/doc/src/registry.rs`, `crates/doc/src/registry/tests.rs`
- Modify/Test: `crates/sync/src/registry.rs`, `crates/sync/tests/{registry_client,registry_edge}.rs`
- Modify/Test: `crates/engine/src/{registry,diff_sync,doc_host}.rs`
- Modify/Test: `crates/engine/tests/{workspace_sync,diff_sync_churn}.rs`
- Modify/Test: relevant `edge/src/registry-*` and workerd tests

**Interfaces:**
- Consumes: existing registry row/ack wire shapes
- Produces: contiguous cursor application, retryable malformed ack, server-truth cleanup gate, lenient Chat config decode

- [ ] **Step 1: RED — cursor contiguity**

Port the literal fixtures and expectations from upstream tests `own_push_ack_never_advances_the_cursor`, `broadcast_seq_gap_applies_rows_but_holds_the_cursor`, and `pre_epoch_snapshots_resync_in_full_once`. Run the narrow doc/sync tests and confirm failure against the preserved fork implementation.

- [ ] **Step 2: GREEN — cursor contiguity**

Port the compatible `apply_rows`, acknowledgement, and resync-epoch behavior from `1fc68435`; rerun the same tests.

- [ ] **Step 3: RED→GREEN — server truth and acknowledgements**

Port `registry_synced` and `unreadable_http_ack_retries_instead_of_stranding_the_batch` tests from `28eb39b6`, run RED, then implement the minimal engine/sync gates and rerun GREEN.

- [ ] **Step 4: RED→GREEN — unknown harness and diff churn**

Port `future_harness_chat_rows_stay_visible_without_their_config` from `5306be20` plus the bounded capture tests from `74f4abef`/`2119bf0c`; implement lenient optional config decoding and reconciliation suppression.

- [ ] **Step 5: Commit the independently verifiable block**

```bash
git add crates/doc crates/sync crates/engine edge
git commit -m "fix(sync): port upstream registry durability fixes"
```

### Task 3: Native OpenCode and Workers compatibility

**Files:**
- Modify/Create/Test: `crates/harness/src/opencode/`
- Modify/Test: `crates/harness/src/{lib,acp/mod,acp/subagent_opencode}.rs`
- Modify/Test: `crates/harness/tests/{opencode,acp,acp_stall,real_quiet_survey}.rs`
- Modify/Test: `crates/ui/src/{pickers,settings/harnesses}.rs`

**Interfaces:**
- Consumes: OpenCode HTTP/SSE protocol and existing `AgentEvent` normalization
- Produces: bounded native driver and connected-provider model catalog with tagged Workers child events

- [ ] **Step 1: RED — native startup and deadlines**

Port upstream fake-server tests for readiness, event subscription before prompt, bounded HTTP calls, failure surfacing, resume, steer, and completion from `bf634445..a401432f`. Run `cargo test -p zeron-harness opencode` and confirm the native-path expectations fail.

- [ ] **Step 2: GREEN — native runtime**

Port the native OpenCode driver modules and wire them through the fork's existing harness descriptor without removing Workers MCP injection or parent Chat identity.

- [ ] **Step 3: RED→GREEN — catalog and subagents**

Port connected-provider fixtures from `4fd35579` and parent/child attribution regressions from `5019dc19`, `18987da3`, and `569793ed`; preserve richer fork tests where they cover the same mutation.

- [ ] **Step 4: Commit**

```bash
git add crates/harness crates/ui/src/pickers.rs crates/ui/src/settings/harnesses.rs
git commit -m "feat(harness): port native OpenCode runtime"
```

### Task 4: Transcript and Changes navigation

**Files:**
- Modify/Test: `crates/doc/src/parts.rs`
- Modify/Test: `crates/ui/src/{transcript,changes}.rs`
- Modify/Test: `crates/ui/src/markdown/`
- Modify: `openspec/specs/{sticky-turn-headers,turn-step-tool-groups}/spec.md` only at archive

**Interfaces:**
- Consumes: fork `MessagePart::Reasoning`, TurnSteps, transcript row projection, Changes tabs/comments
- Produces: copy/viewport/thinking parity and sticky Changes headers without regressing sticky Chat turns

- [ ] **Step 1: Characterize existing Reasoning**

Run existing `reasoning_*`, workflow grouping, and transcript projection tests. Compare mutations covered by upstream `aa9f8bf1`; add only missing behavioral tests, and do not replace the fork implementation when already equivalent.

- [ ] **Step 2: RED→GREEN transcript behavior**

Port literal copy, viewport, live-anchor, and Thinking group fixtures from `v0.2.19..v0.2.28`; run focused doc/UI tests before implementation hunks.

- [ ] **Step 3: RED→GREEN sticky Changes headers**

Port the state/boundary tests from `d9744836..b3fa5187`, then adapt the implementation to the fork's right-pane/tab registry and comments.

- [ ] **Step 4: Commit**

```bash
git add crates/doc/src/parts.rs crates/ui/src/transcript.rs crates/ui/src/changes.rs crates/ui/src/markdown
git commit -m "feat(ui): port upstream transcript and changes navigation"
```

### Task 5: Themes, typography, picker, sidebar, and shortcuts

**Files:**
- Create/Test: `crates/theme/`
- Modify: `Cargo.toml`, `Cargo.lock`, `THIRD_PARTY_NOTICES.md`
- Modify/Test: `crates/ui/src/{theme,theme_library,typography,pickers,composer,shell,state}.rs`
- Modify/Test: `crates/ui/src/settings/{appearance,shortcuts}.rs`
- Modify/Test: `crates/proto/src/entities.rs`, `crates/engine/src/{registry,rpc}.rs`
- Add: compatible Geist font assets and licenses

**Interfaces:**
- Consumes: GPUI appearance state, harness/model catalogs, durable Chat source context
- Produces: local theme registry, interface typography, scalable picker, sidebar views, Chat shortcuts

- [ ] **Step 1: RED→GREEN theme crate**

Port upstream theme resolution/import fixtures with literal expected colors and persistence shapes; add `zeron-theme`, `json5`, and `plist` only after RED. Preserve license notices and keep GPL Zed crates excluded.

- [ ] **Step 2: RED→GREEN typography**

Port compatibility filtering, bundled/system family, persistence, and remeasurement tests. Ensure code/diff/terminal surfaces continue using monospaced fonts.

- [ ] **Step 3: RED→GREEN picker/completions**

Port `ModelRowsKey` catalog-scale tests, harness tabs, provider attribution, favorites/loading states, picker-open navigation, and full-width scrollable completion fixtures.

- [ ] **Step 4: RED→GREEN sidebar/shortcuts**

Port `ConversationSourceContext`, organization/sort, archive shortcut, `JUMP_SLOTS`, modifier conflict, and exact visible-row target tests. Keep fork-only Workers, Usage, device, and project rows.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml Cargo.lock THIRD_PARTY_NOTICES.md crates/theme crates/proto crates/engine crates/ui
git commit -m "feat(ui): port upstream themes and chat navigation"
```

### Task 6: DOX and final verification

**Files:**
- Modify: affected `AGENTS.md` files and Child DOX Indexes
- Modify: `ARCHITECTURE.md`, `CONTEXT.md`, `docs/PARITY.md`
- Modify: `fork_changelog.md`, `fork_sync_report.md`
- Modify: OpenSpec tasks as each block completes

**Interfaces:**
- Consumes: completed capability blocks
- Produces: clean review branch with mechanical and visual evidence

- [ ] **Step 1: DOX pass**

Read the full AGENTS chain for every changed path, update each nearest owner and parent index, remove stale statements, and align Test Coverage Matrices with scenario `Test:` stamps.

- [ ] **Step 2: Focused edge and packaging gates**

```bash
npm -C edge test
npm -C edge run typecheck
bash -n scripts/package-linux.sh scripts/package-macos.sh
```

- [ ] **Step 3: Complete gate once**

```bash
cargo test
cargo build -p zeron
cargo fmt --all -- --check
openspec validate sync-upstream-v0-2-29 --strict
git diff --check
```

- [ ] **Step 4: Native smoke and report**

Run `scripts/dev-demo.sh` and inspect appearance import, typography reflow, model picker, Chat organization/jump/archive, transcript Thinking/copy, composer popups, and sticky Changes headers. Mark unexecuted visual scenarios `human-needed` rather than inferring success.

- [ ] **Step 5: Final review commit and handoff**

Commit report/DOX/OpenSpec task completion, confirm `git status --porcelain` is empty, and leave `/Users/guilhermevarela/Documents/Projetos/SelfHosting/comet-upstream-v0.2.29` retained. Do not promote or push without explicit approval.
