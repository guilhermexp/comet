# Blobatar Subagent Avatars Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the Codex subagent avatar atlas with committed Blobatar SVG assets while preserving stable per-subagent selection.

**Architecture:** Generate Blobatar 2.4.0 SVGs offline and commit them as ordinary GPUI assets. Keep runtime selection in Rust and eliminate theme pairs, network access, and JavaScript runtime dependencies.

**Tech Stack:** Rust, GPUI, SVG assets, Cargo tests, Bun used only for offline asset generation.

**Spec:** `docs/plans/2026-08-23-blobatar-subagent-avatars-design.md`

## Global Constraints

- Blobatar source is pinned to revision `f3d691ff5ca0aa4d00f6c4cfd48f4f3ff16c76e2` and package version `2.4.0`.
- Runtime must remain offline and Rust-only.
- Avatar identity remains stable from the subagent row ID.
- No dark/light asset pairs.
- Preserve unrelated working-tree changes.

---

### Task 1: Blobatar asset contract

**Files:**
- Modify: `crates/ui/src/details_sidebar/subagent_avatars.rs`
- Modify: `crates/ui/src/details_sidebar/view.rs`
- Modify: `crates/ui/src/icons.rs`

**Interfaces:**
- Consumes: `row.id: String`
- Produces: `blobatar_subagent_avatar_path(seed: &str) -> &'static str`

- [ ] **Step 1: Write failing tests**

Add literal expectations that the selector returns `icons/subagents/blobatar/23.svg`
for `subagent-1`, returns the same path for both themes at the call site, and
that the embedded asset set contains 28 GPUI-renderable SVGs.

- [ ] **Step 2: Verify RED**

Run: `cargo test -p zeron-ui blobatar_subagent_avatar --lib`

Expected: FAIL because the Blobatar selector and asset directory do not exist.

- [ ] **Step 3: Add generated assets and minimal implementation**

Generate 28 static SVGs from `comet-subagent-0` through
`comet-subagent-27`, replace the selector with a single-path atlas, rename the
embedded loader, and update the row call site.

- [ ] **Step 4: Verify GREEN**

Run: `cargo test -p zeron-ui blobatar_subagent_avatar --lib`

Expected: PASS.

### Task 2: Provenance and complete validation

**Files:**
- Modify: `crates/ui/build.rs`
- Modify: `THIRD_PARTY_NOTICES.md`
- Delete: `crates/ui/assets/icons/subagents/codex/*.svg`
- Create: `crates/ui/assets/icons/subagents/blobatar/*.svg`

**Interfaces:**
- Consumes: the committed 28-file Blobatar atlas
- Produces: embedded GPUI assets under `icons/subagents/blobatar/`

- [ ] **Step 1: Record provenance and remove obsolete naming**

Add the Blobatar MIT notice and pin, rename generated loader artifacts, and
remove all Codex avatar paths and assets.

- [ ] **Step 2: Run focused and full gates**

Run:

```bash
cargo test -p zeron-ui --lib
cargo build -p zeron-ui
```

Expected: both commands pass with every generated SVG parsed by GPUI.

- [ ] **Step 3: Self-check**

Inspect `git diff --check`, confirm no `icons/subagents/codex` references remain,
and report the final asset size reduction.

