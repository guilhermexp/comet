# Composer Corner Radius Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Match only the main composer pill to the 12px user-message card radius.

**Architecture:** Keep the existing GPUI composer structure and theme chrome intact. Introduce one named radius constant used by the main pill and covered by a focused unit contract.

**Tech Stack:** Rust, GPUI, Cargo tests.

## Global Constraints

- Main composer radius must be exactly 12px.
- Do not change height, padding, colors, border, shadow, controls, or behavior.
- Do not change the question panel or other rounded elements.

---

### Task 1: Refine the main composer radius

**Files:**
- Modify: `crates/ui/src/composer.rs`
- Test: `crates/ui/src/composer.rs`

**Interfaces:**
- Consumes: GPUI `Pixels` via `px`.
- Produces: `COMPOSER_CORNER_RADIUS: f32` used by the main composer pill.

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn main_composer_matches_the_user_message_card_radius() {
    assert_eq!(COMPOSER_CORNER_RADIUS, 12.0);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p zeron-ui composer::tests::main_composer_matches_the_user_message_card_radius --no-default-features`

Expected: compilation fails because `COMPOSER_CORNER_RADIUS` does not exist.

- [ ] **Step 3: Write minimal implementation**

Add near the composer layout constants:

```rust
const COMPOSER_CORNER_RADIUS: f32 = 12.0;
```

Change only the main pill construction:

```rust
let pill = div()
    .rounded(px(COMPOSER_CORNER_RADIUS))
```

- [ ] **Step 4: Run focused and full composer tests**

Run:

```bash
cargo test -p zeron-ui composer::tests::main_composer_matches_the_user_message_card_radius --no-default-features
cargo test -p zeron-ui composer::tests --no-default-features
cargo fmt --all -- --check
git diff --check
cargo check --workspace
cargo build -p zeron
```

Expected: all commands exit successfully; existing repository warnings may remain unchanged.

- [ ] **Step 5: Validate visually and commit**

Restart `RUST_LOG=warn cargo run -p zeron`, capture the Zeron window, and verify that only the composer is slightly less rounded.

```bash
git add crates/ui/src/composer.rs docs/superpowers/plans/2026-08-21-composer-corner-radius.md
git commit -m "refine composer corner radius"
```
