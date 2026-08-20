# Composer Footer Model Controls Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move model and effort controls from inside the Orchestrator input to the footer immediately before the branch control.

**Architecture:** Extract the incumbent model/effort renderer into `Pickers::render_model_controls`, preserving all behavior and visual tokens. `render_footer` composes checkout on the left and model/effort/optional branch on the right, while Composer removes its inline picker mounts.

**Tech Stack:** Rust, GPUI, existing picker/popover primitives, source-contract unit tests.

## Global Constraints

- Right-side order is model, effort, branch.
- Branch remains the extreme-right control.
- Existing model/effort pills and popovers are reused without visual redesign.
- Attachment and send controls remain inside the input.
- Model/effort remain accessible when no git branch exists.
- No commit is created without separate user authorization.

---

### Task 1: Move model controls into the footer

**Files:**
- Modify: `crates/ui/src/pickers.rs`
- Modify: `crates/ui/src/composer.rs`
- Test: `crates/ui/src/pickers.rs`
- Test: `crates/ui/src/composer.rs`

**Interfaces:**
- Produces: `Pickers::render_model_controls(&mut self, &mut Window, &mut Context<Self>) -> AnyElement`.
- Changes: `Pickers::render_footer` accepts `&mut Window` and always renders model controls.

- [ ] **Step 1: Write failing source-contract tests**

Assert that Composer contains no `self.pickers.clone()` mount and that `render_footer` calls `render_model_controls` before appending the branch control.

- [ ] **Step 2: Run the focal UI tests and verify RED**

Run: `cargo test -p zeron-ui picker_controls_move_to_footer`

Expected: FAIL because picker controls are still inside Composer and absent from the footer.

- [ ] **Step 3: Extract model/effort rendering**

Move the existing loading, focus, labels, pills, and model/traits overlays into `render_model_controls`; keep the same `trigger_chip` calls and above-end anchoring.

- [ ] **Step 4: Compose the footer right cluster**

Render `model_controls` before the existing branch chip/label for git projects and by itself for non-git projects. Keep checkout on the left.

- [ ] **Step 5: Remove inline composer mounts**

Delete the two `self.pickers.clone()` children from expanded and compact action rows while keeping their flex spacer, attachment, and send controls.

- [ ] **Step 6: Run UI tests and gates**

Run:

```bash
cargo test -p zeron-ui picker_controls_move_to_footer
cargo check --workspace
cargo fmt --all -- --check
git diff --check
```

- [ ] **Step 7: Rebuild, restart dev, and visually inspect**

Verify model and effort are absent from the input, appear before branch in the footer, and both menus open above the correct trigger without clipping.
