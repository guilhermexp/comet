# Full-height Utility Pane Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn the shared Terminal/Changes pane into a full-height right column with its own named closeable header and no inset outer card or horizontal titlebar separation.

**Architecture:** Keep `SessionPanels` as the single session-scoped pane state and reuse the persisted shared width. Split the ready shell into a flexible left workspace and a full-height right utility column; the left workspace owns the existing titlebar, sidebar, and conversation card, while the utility column owns its own titlebar-height header and body.

**Tech Stack:** Rust 2024, GPUI, existing Comet motion/theme/icon helpers, Cargo tests.

## Global Constraints

- Terminal and Changes remain mutually exclusive and session-scoped.
- The utility pane reaches the top, right, and bottom window edges.
- Remove the utility pane's inset card radius, outer gutters, and horizontal divider.
- Keep only the thin vertical resize seam between conversation and utility pane.
- Render `Terminal ×` or `Changes ×` in a dedicated first row; Terminal's internal tabs remain in the second row.
- Preserve `Cmd+J`, the existing titlebar toggles, 200 ms width animation, persisted width, PTY state, and diff watching.
- Do not change engine, RPC, PTY, or settings schema behavior.

---

### Task 1: Explicit pane title and close state

**Files:**
- Modify: `crates/ui/src/shell.rs:176-205`
- Test: `crates/ui/src/shell.rs` test module

**Interfaces:**
- Produces: `UtilityPane::label(self) -> &'static str`
- Produces: `SessionPanels::close(&mut self, key: &str) -> bool`
- Consumes: existing `SessionPanels::active` and `SessionPanels::toggle`

- [ ] **Step 1: Write the failing state-contract test**

```rust
#[test]
fn utility_pane_labels_and_close_are_explicit() {
    assert_eq!(UtilityPane::Terminal.label(), "Terminal");
    assert_eq!(UtilityPane::Changes.label(), "Changes");

    let mut panels = SessionPanels::default();
    assert!(panels.toggle("chat-a", UtilityPane::Terminal));
    assert!(panels.close("chat-a"));
    assert_eq!(panels.active("chat-a"), None);
    assert!(!panels.close("chat-a"));
}
```

- [ ] **Step 2: Run the test and verify the missing APIs fail compilation**

Run: `cargo test -p comet-ui utility_pane_labels_and_close_are_explicit -- --nocapture`

Expected: FAIL because `UtilityPane::label` and `SessionPanels::close` do not exist.

- [ ] **Step 3: Add the minimal model APIs**

```rust
impl UtilityPane {
    fn label(self) -> &'static str {
        match self {
            Self::Terminal => "Terminal",
            Self::Changes => "Changes",
        }
    }
}

impl SessionPanels {
    pub fn close(&mut self, key: &str) -> bool {
        self.map.remove(key).is_some()
    }
}
```

Keep the existing `active` and `toggle` methods unchanged.

- [ ] **Step 4: Run the focused state tests**

Run: `cargo test -p comet-ui utility_pane -- --nocapture`

Expected: both the existing exclusivity test and the new title/close test PASS.

- [ ] **Step 5: Commit the state contract**

```bash
git add crates/ui/src/shell.rs
git commit -m "refactor(ui): model utility pane close state"
```

---

### Task 2: Build the full-height utility column

**Files:**
- Modify: `crates/ui/src/shell.rs:858-935`
- Modify: `crates/ui/src/shell.rs:2873-2933`
- Modify: `crates/ui/src/shell.rs:3598-3732`
- Modify: `crates/ui/src/shell/tabs.rs:552-595` comments only where the titlebar ownership description changes

**Interfaces:**
- Consumes: `UtilityPane::label`, `SessionPanels::close`, `Shell::pane_container`, `Shell::titlebar_drag_region`, `header_icon_button`
- Produces: `Shell::close_utility_pane(&mut self, window: &mut Window, cx: &mut Context<Self>)`
- Produces: `Shell::render_utility_header(&mut self, pane: UtilityPane, cx: &mut Context<Self>) -> AnyElement`
- Preserves: `Shell::render_right_pane`, `Shell::right_target`, and the resize action signatures

- [ ] **Step 1: Add a single close transition used by the header ×**

```rust
fn close_utility_pane(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    let from = self.right_target(cx);
    let key = self.panel_key(cx);
    if !self.panels.close(&key) {
        return;
    }
    self.right_tween = Some(WidthTween::new(from, self.right_target(cx)));
    if let Some(terminal) = self.terminal.clone() {
        terminal.update(cx, |panel, cx| panel.set_open(false, cx));
    }
    window.focus(&self.composer.focus_handle(cx), cx);
    cx.notify();
}
```

This closes whichever pane is active without reusing a pane-specific toggle.

- [ ] **Step 2: Add the owned utility header**

Build a `Theme::TITLEBAR_HEIGHT` row with no bottom border. Use `pane.label()` for the text, `icons::TERMINAL` or `icons::SIDEBAR_MINIMALISTIC` for the leading glyph, and `header_icon_button("close-utility-pane", icons::CLOSE, false, ...)` for ×. Wrap the row with:

```rust
self.titlebar_drag_region("utility-pane-titlebar", bar, cx)
    .into_any_element()
```

The close listener must call `close_utility_pane(window, cx)`. The close control remains occluding through `header_icon_button`, so it cannot start a native window drag.

- [ ] **Step 3: Replace the inset right card with a full-height column**

In `render_right_pane`, compute `active` once, derive its header and body from the same value, and compose:

```rust
div()
    .size_full()
    .relative()
    .flex()
    .flex_col()
    .bg(theme.bg)
    .border_l_1()
    .border_color(theme.border)
    .child(header)
    .child(div().flex_1().min_h_0().overflow_hidden().child(content))
    .child(handle)
```

Pass this element directly to `pane_container`. Delete the rounded outer card, `.pb(8)`, `.pr(8)`, and all inset-card comments. Keep the resize handle absolutely positioned on the left edge.

- [ ] **Step 4: Move the right pane outside the global titlebar column**

Replace the ready page's `flex_col(title_bar, body_row)` composition with:

```rust
let left_workspace = div()
    .h_full()
    .flex_1()
    .min_w_0()
    .flex()
    .flex_col()
    .child(title_bar)
    .child(
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_row()
            .child(sidebar)
            .child(sidebar_seam)
            .child(card),
    );

let page = div()
    .size_full()
    .flex()
    .flex_row()
    .child(left_workspace)
    .child(right)
    .child(self.render_titlebar_cluster(cx))
    .children(overlays);
```

The right pane now owns the top-right window area. Keep `sidebar_tone` as the existing absolute full-height background.

- [ ] **Step 5: Remove the gap and square the conversation edge at the open seam**

When `right_pane_open(cx)` is true, use a `0px` right margin on the conversation card and set its top-right and bottom-right radii to `0px`; otherwise preserve the current `8px` right gutter and `12px` radius. This leaves one continuous vertical seam instead of the previous card-to-card gap.

- [ ] **Step 6: Compile the shell change**

Run: `cargo check -p comet`

Expected: PASS. Existing future-incompatibility dependency warnings are acceptable; new errors or warnings in modified code are not.

- [ ] **Step 7: Commit the full-height composition**

```bash
git add crates/ui/src/shell.rs crates/ui/src/shell/tabs.rs
git commit -m "feat(ui): extend utility pane to window top"
```

---

### Task 3: Regression and visual verification

**Files:**
- Verify: `crates/ui/src/shell.rs`
- Verify: `crates/ui/src/shell/tabs.rs`
- Verify: `crates/ui/src/terminal/panel.rs`

**Interfaces:**
- Consumes: final `comet-ui` behavior and `scripts/dev-demo.sh`
- Produces: evidence that layout, keyboard, focus, resize, and engine connectivity still work

- [ ] **Step 1: Run the full UI test suite**

Run: `cargo test -p comet-ui`

Expected: all tests PASS.

- [ ] **Step 2: Build the desktop binary**

Run: `cargo build -p comet`

Expected: PASS with no new project warnings.

- [ ] **Step 3: Restart the persistent demo**

Stop the existing `comet-dev` process through the harness, then start `/bin/zsh scripts/dev-demo.sh` from the repository root. Wait for the `opening comet` readiness log.

Expected: the Comet window opens with seeded chats and a responsive local engine.

- [ ] **Step 4: Exercise the terminal path**

Open Terminal from the titlebar, confirm the `Terminal ×` header occupies the top-right row, verify `Terminal 1` is directly below it, resize from the vertical seam, close with ×, reopen with `Cmd+J`, and type a harmless shell command.

Expected: the pane reaches top/right/bottom edges, has no outer rounded card, outer gutters, or horizontal divider; command output appears and the composer regains focus after close.

- [ ] **Step 5: Exercise the Changes path and session state**

Switch Terminal → Changes, close from the `Changes ×` header, reopen, then switch to another chat and back.

Expected: switching content does not collapse width; only one pane is visible; each chat restores its own active pane; the resize width remains persisted.

- [ ] **Step 6: Verify local RPC health**

Run: `cargo run -q -p comet-rpc --example rpc_probe -- ws://127.0.0.1:27921 LocalDevice '{}'`

Expected: one JSON object containing a non-empty `deviceId`.

- [ ] **Step 7: Capture final visual evidence**

Capture the Comet window with Terminal open at desktop width and inspect it against the approved reference.

Expected: full-height right column, named top header with ×, Terminal tabs on the second row, and only the thin vertical resize seam between columns.
