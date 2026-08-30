# Native Runtime Integrity Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task with review checkpoints.

**Goal:** Remove the observed GPUI reentrant borrow and stop local writers from emitting the malformed Chat Transcript part while retaining privacy-safe salvage.

**Architecture:** Diagnose before repair. Instrument only callback identity and JSON structure, reproduce each defect, then fix the originating UI mutation and transcript writer. Readers remain strict-first with defensive salvage.

**Tech Stack:** Rust 2024, gpui, serde/serde_json, tracing, native headed smoke.

**Spec:** `openspec/changes/repair-native-runtime-integrity/`

**Global Constraints:** Never log transcript content or tool payloads. Do not weaken borrow/schema checks. Re-read the full DOX chain for every concrete file immediately before editing because UI files are concurrently dirty.

## Task 1: Localize the GPUI reentrant callback

**Files:**
- Inspect: `crates/ui/src/transcript.rs`
- Inspect: `crates/ui/src/mermaid_preview.rs`
- Inspect: `crates/ui/src/workers/{menu_bar,session_gallery,session_menu,workspace_open_menu}.rs`
- Modify: the smallest confirmed UI module and its nearest test module

- [ ] Snapshot `git diff --` for candidate files and do not overwrite concurrent work.
- [ ] Search nested `update`, `update_window`, `defer`, and event callbacks; add static callback labels only around candidates exercised near the logged timestamp.
- [ ] Run a headed debug build with `RUST_LOG=gpui::window=debug,zeron_ui=debug` and reproduce through the real preview/menu interaction until one label immediately precedes `RefCell already borrowed`.
- [ ] Add a focused failing regression for the confirmed pure action/state transition where possible; otherwise preserve the headed reproduction as the RED gate.

Diagnostic shape (no user content):

```rust
tracing::debug!(callback = "mermaid_preview_action", "native ui callback entered");
```

## Task 2: Remove the confirmed nested mutation

**Files:**
- Modify: the confirmed module from Task 1
- Modify: its existing test module

- [ ] Move pure state calculation outside the nested entity/window update.
- [ ] Apply the result once through the `Context` already owned by the event callback; if the framework requires a later boundary, use the existing GPUI defer mechanism with a weak handle.
- [ ] Do not replace checked borrowing with unchecked access or ignore the returned error.
- [ ] Run the focused test and repeat the real interaction at least ten times with no borrow error.

Preferred transition shape:

```rust
let next = this.preview_state.transition(action);
this.preview_state = next;
cx.notify();
```

If the event has only `&mut App`, defer one weak update instead of nesting it inside another update callback.

## Task 3: Add privacy-safe transcript failure metadata

**Files:**
- Modify: `crates/doc/src/schema.rs`
- Modify: tests in `crates/doc/src/schema.rs`

- [ ] Add a RED test using sentinel values such as `SECRET_TRANSCRIPT_CONTENT` and malformed nested JSON missing `id`.
- [ ] Introduce a small structural descriptor containing only a normalized JSON path and allowlisted part kind.
- [ ] Include that descriptor in the existing strict-parse/salvage warning; never serialize the offending JSON.
- [ ] Assert the formatted diagnostic contains `parts[0].id` and the part kind, and excludes every sentinel value.

Target helper contract:

```rust
struct StructuralParseFailure {
    field_path: &'static str,
    part_kind: Option<&'static str>,
}
```

The implementation may derive the path from `serde_path_to_error` only if that dependency is already compatible; otherwise inspect required keys with an allowlist before salvage.

## Task 4: Trace and fix the malformed writer

**Files:**
- Inspect: all constructors/serializers found by `rg 'TranscriptPart|parts.*id|serde_json::json!' crates apps`
- Modify: the one confirmed writer and its test
- Modify: `crates/doc/src/schema.rs` only if a shared validated constructor already belongs there

- [ ] Reproduce the structural signature from entry `fe6eafcc-8845-4c8f-991e-2b822e4f56e2` without copying its content into fixtures.
- [ ] Trace from the part kind to the writer; add a RED producer test that strict-parses its output.
- [ ] Populate the canonical part id at the source using the existing id convention; do not generate fallback ids in the reader.
- [ ] Prove strict parsing succeeds and the existing salvage suite still passes.

## Task 5: DOX and full verification

**Files:**
- Modify: nearest affected `AGENTS.md` files and parent Child DOX indexes only when their durable contracts changed
- Modify: `openspec/changes/repair-native-runtime-integrity/tasks.md`

- [ ] Run `cargo fmt --all -- --check`.
- [ ] Run focused `zeron-doc` and `zeron-ui` tests.
- [ ] Run `cargo test` and `cargo build -p zeron`.
- [ ] Run the bounded headed smoke and assert logs contain neither target error nor transcript payload sentinels.
- [ ] Validate the OpenSpec strictly and mark evidenced tasks complete.
