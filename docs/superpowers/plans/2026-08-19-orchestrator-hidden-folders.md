# Orchestrator Hidden Folders Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Show hidden directories in the Orchestrator folder picker without exposing ordinary files as navigable rows.

**Architecture:** Change the engine's single-level folder listing, which is the shared source for local and remote folder pickers. The UI already filters the listing to directories and therefore needs no structural change.

**Tech Stack:** Rust, Tokio integration tests, GPUI consumer.

## Global Constraints

- Hidden directories are always visible.
- Ordinary files remain non-navigable in the folder picker.
- Existing sorting, repository detection, timeout, and entry-cap behavior remain unchanged.
- No commit is created without separate user authorization.

---

### Task 1: Include hidden directories in folder listings

**Files:**
- Modify: `crates/engine/src/repos.rs`
- Test: `crates/engine/tests/m5_repos_diffs_terminals.rs`

**Interfaces:**
- Consumes: `Repos::list_folders` and `list_folders_blocking`.
- Produces: the existing `FolderListing` shape with dot-prefixed directories included.

- [ ] **Step 1: Write the failing test**

Change `folder_lister_flags_and_ordering` to expect directory-first order containing `.hidden`:

```rust
assert_eq!(names, vec![".hidden", "alpha", "beta", "aaa.txt"]);
let hidden = listing.entries.iter().find(|entry| entry.name == ".hidden").unwrap();
assert!(hidden.is_dir && !hidden.is_repo);
```

- [ ] **Step 2: Run the focal test and verify RED**

Run: `cargo test -p zeron-engine --test m5_repos_diffs_terminals folder_lister_flags_and_ordering`

Expected: FAIL because `.hidden` is absent.

- [ ] **Step 3: Implement the minimal behavior**

Remove the `name.starts_with('.')` early-continue from `list_folders_blocking`; do not alter the listing shape or sorting.

- [ ] **Step 4: Run the focal test and verify GREEN**

Run: `cargo test -p zeron-engine --test m5_repos_diffs_terminals folder_lister_flags_and_ordering`

Expected: PASS.

- [ ] **Step 5: Run proportional gates**

Run:

```bash
cargo test -p zeron-engine --test m5_repos_diffs_terminals
cargo check --workspace
cargo fmt --all -- --check
git diff --check
```

Expected: all commands succeed; pre-existing Objective-C cfg warnings may remain in the workspace check.
