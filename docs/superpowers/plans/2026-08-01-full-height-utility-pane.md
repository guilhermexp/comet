# Unified Utility Tab Strip Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: use
> `superpowers:executing-plans` and execute each checkbox in order.

> **Shipped.** The conversation-titlebar contract in this plan is superseded by
> `2026-08-01-utility-panel-toggle.md`: the Changes button is now the
> utility-panel toggle.

**Goal:** Replace the duplicated utility-pane header and nested Terminal tab
bar with one top strip containing Terminal sessions and Changes as sibling
tabs.

**Architecture:** `SessionPanels` owns session-scoped column visibility, the
active tab kind, and whether Changes is open. `TerminalPanel` remains the sole
owner of terminal identity, order, PTYs, and drag behavior; it exposes its tab
group separately from its body and emits small chrome events to `Shell`.
`Shell` composes terminal tabs, the Changes tab, the `+` menu, and the collapse
control into one 40px strip.

**Tech Stack:** Rust 2024, GPUI, existing Comet popover/motion/theme helpers,
Cargo tests.

## Global Constraints

- Keep the current full-height lateral placement and persisted shared width.
- Terminal and Changes tabs may remain open at the same time; only one body is
  active.
- Keep Terminal PTY identity, ordering, replay, input, resize, and reconnect
  behavior unchanged.
- Keep Changes diff watching and rendering unchanged.
- The `+` menu contains only Terminal and Changes.
- Keep the existing conversation-titlebar buttons and `Cmd+J`.
- Do not add Browser, Files, Side Chat, engine, RPC, or settings-schema work.

---

### Task 1: Model persistent sibling-tab state

**Files:**
- Modify: `crates/ui/src/shell.rs`
- Test: `crates/ui/src/shell.rs` test module

**Interfaces:**
- Replace the map value `UtilityPane` with a per-session state containing
  `visible`, `active`, and `changes_open`.
- Add explicit operations for showing Terminal, showing Changes, hiding the
  column, closing Changes, and reconciling terminal presence.
- Keep all state transitions pure and unit-testable.

- [ ] **Step 1: Write failing state-transition tests**

Cover these observable contracts:

```rust
#[test]
fn utility_tabs_remain_open_when_selection_changes() {
    let mut panels = SessionPanels::default();
    panels.show_changes("chat-a");
    panels.show_terminal("chat-a");

    let state = panels.get("chat-a");
    assert!(state.visible);
    assert_eq!(state.active, UtilityPane::Terminal);
    assert!(state.changes_open);
}

#[test]
fn closing_active_changes_falls_back_to_terminal() {
    let mut panels = SessionPanels::default();
    panels.show_changes("chat-a");
    panels.close_changes("chat-a", true);

    let state = panels.get("chat-a");
    assert!(state.visible);
    assert_eq!(state.active, UtilityPane::Terminal);
    assert!(!state.changes_open);
}

#[test]
fn removing_the_last_open_tab_collapses_the_column() {
    let mut panels = SessionPanels::default();
    panels.show_terminal("chat-a");
    panels.reconcile_terminal_presence("chat-a", false);
    assert!(!panels.get("chat-a").visible);
}
```

Also assert that hiding/reopening preserves `changes_open`, and that a
remaining Changes tab becomes active when the final Terminal closes.

- [ ] **Step 2: Run the focused tests and observe failure**

Run:

```bash
cargo test -p comet-ui utility_tabs_ -- --nocapture
```

Expected: FAIL because the new state and operations do not exist.

- [ ] **Step 3: Implement the minimal state reducer**

Use a small `ChatPanelState` value with a closed default. State lookup for an
unknown key must not allocate. Mutating operations may lazily insert.

The reducer must encode:

- `show_terminal`: visible + Terminal active; preserve `changes_open`.
- `show_changes`: visible + Changes active + `changes_open = true`.
- `hide`: set only `visible = false`.
- `close_changes(has_terminal)`: remove Changes; select Terminal when present,
  otherwise hide.
- `reconcile_terminal_presence(false)`: if Terminal was active, select Changes
  when open, otherwise hide.

- [ ] **Step 4: Run the focused state tests**

Run:

```bash
cargo test -p comet-ui utility_tabs_ -- --nocapture
```

Expected: PASS.

---

### Task 2: Separate Terminal chrome from Terminal body

**Files:**
- Modify: `crates/ui/src/terminal/panel.rs`
- Modify: `crates/ui/src/shell.rs`
- Test: existing `crates/ui/src/terminal/panel.rs` tests

**Interfaces:**
- Add a crate-visible `TerminalPanelEvent` with `Changed { chat }` and
  `Activated { chat }`.
- Implement `EventEmitter<TerminalPanelEvent>` for `TerminalPanel`.
- Expose `has_tabs(chat)`, `open_new_tab`, and `render_tab_group`.
- Make `Render for TerminalPanel` render only the active terminal body.

- [ ] **Step 1: Add the terminal-to-shell event contract**

Emit `Changed` only for chrome mutations: open, close, reorder, and drag-state
changes. Emit `Activated` when a terminal tab is clicked. Do not emit chrome
events for terminal output, cursor movement, or resize frames.

- [ ] **Step 2: Expose the terminal tab group**

Refactor the current `render_tab_bar` into a group that retains:

- Fixed-width terminal tabs.
- Selection, close, middle-click, drag ghost, and reorder behavior.
- Active/exited styling.

The group must omit:

- The 40px outer bar.
- Padding owned by the shared strip.
- The `+` button.
- The collapse chevron.

Pass a `terminal_active` boolean so no terminal tab appears selected while
Changes owns the body.

- [ ] **Step 3: Render only the terminal body from the entity**

Keep the existing focus, keyboard, scroll, emulator, and empty-chat behavior.
Remove only the `.child(self.render_tab_bar(...))` layer.

- [ ] **Step 4: Subscribe Shell to terminal chrome events**

Store the lazy subscription beside the lazy `TerminalPanel` entity.

- `Changed` repaints the shared strip and reconciles whether the current chat
  still has terminal tabs.
- `Activated` makes Terminal the visible active utility tab without creating a
  new PTY.

When the final Terminal closes, Changes becomes active if open; otherwise the
column animates closed.

- [ ] **Step 5: Run terminal and state tests**

Run:

```bash
cargo test -p comet-ui terminal -- --nocapture
cargo test -p comet-ui utility_tabs_ -- --nocapture
```

Expected: PASS.

---

### Task 3: Compose the single shared strip

**Files:**
- Modify: `crates/ui/src/shell.rs`
- Reuse: `crates/ui/src/popover.rs`

**Interfaces:**
- Replace `render_utility_header` with `render_utility_tab_strip`.
- Add Shell state for whether the utility `+` menu is open.
- Keep `render_right_pane` as the full-height column owner.

- [ ] **Step 1: Add explicit Shell actions**

Implement:

- Open/select Terminal.
- Create a new Terminal from `+`.
- Open/select Changes.
- Close Changes.
- Hide/reopen the utility column.

The conversation-titlebar buttons route through these same operations.

- [ ] **Step 2: Render one 40px top strip**

Compose, left to right:

1. Terminal tab group from `TerminalPanel`, when terminal tabs exist.
2. One `Changes ×` tab when `changes_open` and git is available.
3. The `+` trigger.
4. Flexible draggable titlebar space.
5. The existing collapse chevron.

Interactive children must stop propagation so selecting, closing, dragging, or
opening the menu cannot start a native window drag.

- [ ] **Step 3: Add the `+` menu**

Use existing `popover::popover_card`, `popover::menu_row`, and
`popover::anchored_menu`. Render two rows:

- Terminal — creates and selects a new terminal.
- Changes — opens/selects the singleton Changes tab.

Dismiss on outside mouse-down and after either selection.

- [ ] **Step 4: Remove the duplicated header**

Delete the `Terminal ×` / `Changes ×` header renderer. The right pane becomes:

```rust
div()
    .size_full()
    .flex()
    .flex_col()
    .child(shared_tab_strip)
    .child(active_body)
```

Keep the left resize handle, border seam, width tween, and full-height
composition unchanged.

- [ ] **Step 5: Compile the desktop app**

Run:

```bash
cargo check -p comet
```

Expected: PASS with no new project warnings.

---

### Task 4: Verify the corrected behavior

- [ ] **Step 1: Run the complete UI suite**

```bash
cargo test -p comet-ui
```

Expected: all tests PASS.

- [ ] **Step 2: Build the desktop binary**

```bash
cargo build -p comet
```

Expected: PASS.

- [ ] **Step 3: Exercise the live desktop flow**

Restart `scripts/dev-demo.sh`, then:

1. Open Terminal.
2. Use `+` to create Terminal 2.
3. Use `+` to open Changes.
4. Select Terminal 1, Terminal 2, and Changes in sequence.
5. Close each active tab and verify neighbor fallback.
6. Reopen/collapse with `Cmd+J`.
7. Resize the left seam and switch chats.
8. Type a harmless command in a terminal.

Expected:

- Exactly one top strip.
- Terminal tabs and Changes remain visible beside one another.
- No `Terminal ×` or `Changes ×` row above the tabs.
- Only the active body is rendered.
- The last tab closes the column.
- PTY input/output, diff rendering, focus, width, and per-chat state remain
  functional.

- [ ] **Step 4: Verify local RPC health**

```bash
cargo run -q -p comet-rpc --example rpc_probe -- \
  ws://127.0.0.1:27921 LocalDevice '{}'
```

Expected: one JSON object containing a non-empty `deviceId`.

- [ ] **Step 5: Capture final visual evidence**

Capture the Comet window with two Terminal tabs and Changes visible. Compare it
to the approved references: one shared row at the top of the lateral panel,
with each component opening beside the others.
