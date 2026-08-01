# Utility Panel Toggle Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the external Changes control with a state-aware toggle for the whole right utility panel and give Changes a distinct diff icon.

**Architecture:** `SessionPanels` decides whether hidden utility tabs can be restored for the selected session. The existing `utility_add_menu_open` flag drives either the in-panel `+` chooser or the external launcher chooser, depending on panel visibility, so both menus cannot render together. Terminal existence remains derived from `TerminalPanel::has_tabs`; Changes existence remains in `ChatPanelState::changes_open`.

**Tech Stack:** Rust 2024, GPUI, embedded SVG assets, Cargo tests.

## Global Constraints

- Keep the external Terminal button and the `Cmd+J` behavior unchanged.
- Keep `Cmd+B` as the direct Changes shortcut.
- The generic panel button uses the existing `SIDEBAR_MINIMALISTIC` icon.
- Changes uses a new `DIFF` icon in its tab and both utility chooser menus.
- Hiding the utility panel must not close Terminal sessions or remove Changes.
- Reopening restores the last valid active tab and every existing utility tab.
- With no utility tabs, the external button opens the Terminal/Changes chooser without opening an empty panel.
- No new dependency and no settings-schema change.

---

### Task 1: Restore hidden utility tabs safely

**Files:**
- Modify: `crates/ui/src/shell.rs:185-267`
- Test: `crates/ui/src/shell.rs:4081-4160`

**Interfaces:**
- Consumes: `ChatPanelState { visible, active, changes_open }` and `has_terminal: bool` from `TerminalPanel::has_tabs`.
- Produces: `SessionPanels::restore(&mut self, key: &str, has_terminal: bool) -> bool`.

- [ ] **Step 1: Add failing reducer tests**

Add these cases to the existing `shell.rs` test module:

```rust
#[test]
fn utility_panel_restore_keeps_the_last_valid_tab() {
    let mut panels = SessionPanels::default();
    panels.show_terminal("chat-a");
    panels.show_changes("chat-a");
    panels.hide("chat-a");

    assert!(panels.restore("chat-a", true));
    assert_eq!(panels.active("chat-a"), Some(UtilityPane::Changes));
    assert!(panels.get("chat-a").changes_open);
}

#[test]
fn utility_panel_restore_falls_back_to_an_available_tab() {
    let mut panels = SessionPanels::default();
    panels.show_changes("chat-a");
    panels.show_terminal("chat-a");
    panels.hide("chat-a");

    assert!(panels.restore("chat-a", false));
    assert_eq!(panels.active("chat-a"), Some(UtilityPane::Changes));
}

#[test]
fn utility_panel_restore_rejects_an_empty_panel() {
    let mut panels = SessionPanels::default();
    assert!(!panels.restore("chat-a", false));
    assert_eq!(panels.active("chat-a"), None);
}
```

- [ ] **Step 2: Run the reducer tests and confirm failure**

Run:

```bash
cargo test -p comet-ui utility_panel_restore -- --nocapture
```

Expected: compilation fails because `SessionPanels::restore` does not exist.

- [ ] **Step 3: Implement the minimal restore reducer**

Add to `impl SessionPanels`:

```rust
fn restore(&mut self, key: &str, has_terminal: bool) -> bool {
    let state = self.map.entry(key.to_string()).or_default();
    let active_exists = match state.active {
        UtilityPane::Terminal => has_terminal,
        UtilityPane::Changes => state.changes_open,
    };

    if !active_exists {
        if has_terminal {
            state.active = UtilityPane::Terminal;
        } else if state.changes_open {
            state.active = UtilityPane::Changes;
        } else {
            state.visible = false;
            return false;
        }
    }

    state.visible = true;
    true
}
```

- [ ] **Step 4: Run the reducer tests**

Run:

```bash
cargo test -p comet-ui utility_panel_restore -- --nocapture
```

Expected: 3 tests pass.

- [ ] **Step 5: Commit the reducer**

```bash
git add crates/ui/src/shell.rs
git commit -m "refactor(ui): restore hidden utility tabs"
```

---

### Task 2: Add the generic external panel toggle and diff icon

**Files:**
- Create: `crates/ui/assets/icons/diff.svg`
- Modify: `crates/ui/src/icons.rs:50-119`
- Modify: `crates/ui/src/shell.rs:947-1094,3040-3190`
- Modify: `crates/ui/src/shell/tabs.rs:181-230,552-594`

**Interfaces:**
- Consumes: `SessionPanels::restore`, `TerminalPanel::has_tabs`, `Shell::close_utility_pane`, and `Shell::utility_add_menu_open`.
- Produces: `Shell::toggle_utility_panel(&mut self, window: &mut Window, cx: &mut Context<Self>)`, `Shell::render_utility_menu(&mut self, launcher: bool, cx: &mut Context<Self>) -> AnyElement`, and `icons::DIFF`.

- [ ] **Step 1: Add the diff asset and register it**

Create `crates/ui/assets/icons/diff.svg`:

```svg
<svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24"><g fill="none" stroke="currentColor" stroke-linecap="round" stroke-width="1.5"><path d="M4 6h7M4 12h5M4 18h7M18 4v6m-3-3h6m-6 11h6"/></g></svg>
```

Register it beside the other Solar-style controls in `icon_assets!`:

```rust
(DIFF, "diff"),
```

- [ ] **Step 2: Extract one shared utility chooser**

Move the Terminal/Changes rows currently built inside `render_utility_tab_strip` into this Shell helper. Static IDs remain distinct for the two possible anchors:

```rust
fn render_utility_menu(&mut self, launcher: bool, cx: &mut Context<Self>) -> AnyElement {
    let theme = Theme::of(cx).clone();
    let git_detected = self.space_git_detected(cx);
    let (menu_id, terminal_id, changes_id) = if launcher {
        (
            "utility-launcher-menu",
            "utility-launcher-terminal",
            "utility-launcher-changes",
        )
    } else {
        (
            "utility-add-menu",
            "utility-add-terminal",
            "utility-add-changes",
        )
    };

    popover::popover_card(&theme)
        .id(menu_id)
        .w(px(170.0))
        .on_mouse_down_out(cx.listener(|this, _, _, cx| {
            this.utility_add_menu_open = false;
            cx.notify();
        }))
        .child(
            popover::menu_row(&theme, false, terminal_id)
                .id(terminal_id)
                .on_click(cx.listener(|this, _, window, cx| {
                    cx.stop_propagation();
                    this.create_terminal_tab(window, cx);
                }))
                .child(icon(icons::TERMINAL).size(px(16.0)).text_color(theme.text_muted))
                .child("Terminal"),
        )
        .child(
            popover::menu_row(&theme, false, changes_id)
                .id(changes_id)
                .when(!git_detected, |element| {
                    element.opacity(0.35).cursor(gpui::CursorStyle::Arrow)
                })
                .when(git_detected, |element| {
                    element.on_click(cx.listener(|this, _, _, cx| {
                        cx.stop_propagation();
                        this.show_changes(cx);
                    }))
                })
                .child(icon(icons::DIFF).size(px(16.0)).text_color(theme.text_muted))
                .child("Changes"),
        )
        .into_any_element()
}
```

In `render_utility_tab_strip`, call `render_utility_menu(false, cx)` and keep it anchored to the existing `+` button. Replace both current Changes glyph uses in `shell.rs` with `icons::DIFF`.

- [ ] **Step 3: Implement the state-aware panel toggle**

Add to `impl Shell`:

```rust
fn toggle_utility_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    if self.right_pane_open(cx) {
        self.close_utility_pane(window, cx);
        return;
    }

    let from = self.right_target(cx);
    let key = self.panel_key(cx);
    let has_terminal = self
        .terminal
        .as_ref()
        .is_some_and(|terminal| terminal.read(cx).has_tabs(&key));

    if !self.panels.restore(&key, has_terminal) {
        self.utility_add_menu_open = !self.utility_add_menu_open;
        cx.notify();
        return;
    }

    self.utility_add_menu_open = false;
    match self.active_utility_pane(cx) {
        Some(UtilityPane::Terminal) => {
            let terminal = self.terminal_panel(cx);
            terminal.update(cx, |panel, cx| panel.set_open(true, cx));
            window.focus(&terminal.read(cx).focus_handle(), cx);
        }
        Some(UtilityPane::Changes) => {
            let changes = self.changes_pane(cx);
            changes.update(cx, |changes, cx| changes.ensure_watch(cx));
        }
        None => return,
    }
    self.right_tween = Some(WidthTween::new(from, self.right_target(cx)));
    cx.notify();
}
```

- [ ] **Step 4: Replace the external Changes button**

In `render_session_tab_strip`:

- Remove `git` and `changes_active` locals.
- Keep the existing external Terminal button unchanged.
- Add a generic panel button for every selected space.
- Anchor the shared chooser only when the panel is hidden and `utility_add_menu_open` is true.

Build the menu only while the closed-panel launcher is open, before composing `inner`, then add this structure after the Terminal button:

```rust
let launcher_menu = (has_space
    && active_utility.is_none()
    && self.utility_add_menu_open)
    .then(|| self.render_utility_menu(true, cx));

// ... inside the `inner` builder, after the Terminal button:
.when(has_space, |el| {
    el.child(
        div()
            .relative()
            .size(px(28.0))
            .child(header_icon_button(
                "toggle-utility-panel",
                icons::SIDEBAR_MINIMALISTIC,
                active_utility.is_some(),
                &theme,
                cx.listener(|this, _, window, cx| {
                    this.toggle_utility_panel(window, cx)
                }),
            ))
            .when_some(launcher_menu, |button, menu| {
                button.child(popover::anchored_menu(
                    "utility-launcher-menu-anchor",
                    menu,
                ))
            }),
    )
})
```

Delete the old `toggle-changes` titlebar button. Do not remove the `ToggleChanges` action or its key binding.

- [ ] **Step 5: Compile the new UI contract**

Run:

```bash
cargo check -p comet
```

Expected: success with no new warnings from `comet-ui`; only the existing future-incompatibility dependency notice may remain.

- [ ] **Step 6: Exercise the UI behavior**

Restart `comet-dev`, then verify in the running window:

1. Add `Terminal 1`, `Terminal 2`, and `Changes`.
2. Click the external split-panel button: the entire right column closes.
3. Click it again: all three tabs return and Changes remains selected.
4. Close every utility tab and click the external split-panel button.
5. Confirm the anchored `Terminal / Changes` chooser appears without an empty right column.
6. Confirm Changes now uses the new diff glyph, while the external button keeps the split-panel glyph.

Expected: all six observations match; `Cmd+J` and `Cmd+B` still open their direct destinations.

- [ ] **Step 7: Commit the UI behavior**

```bash
git add crates/ui/assets/icons/diff.svg crates/ui/src/icons.rs crates/ui/src/shell.rs crates/ui/src/shell/tabs.rs
git commit -m "feat(ui): toggle the complete utility panel"
```

---

### Task 3: Update product evidence and run final verification

**Files:**
- Modify: `docs/PARITY.md:14-21`
- Modify: `docs/research/feature-inventory.md:31-44,100-105`

**Interfaces:**
- Consumes: the completed generic utility-panel toggle and `icons::DIFF`.
- Produces: product documentation matching the verified behavior.

- [ ] **Step 1: Update the parity wording**

Record that the external split-panel control hides/restores the complete utility column, opens the chooser when empty, and that Changes has a distinct diff glyph. Remove wording that describes the external control as a Changes toggle.

- [ ] **Step 2: Run the complete UI test and build gates**

Run:

```bash
cargo test -p comet-ui
cargo build -p comet
cargo run -q -p comet-rpc --example rpc_probe -- ws://127.0.0.1:27921 LocalDevice '{}'
git diff --check
```

Expected:

- `comet-ui`: all tests pass.
- `comet`: debug build succeeds.
- RPC probe returns JSON containing a non-empty `deviceId`.
- `git diff --check` prints nothing.

- [ ] **Step 3: Commit documentation**

```bash
git add docs/PARITY.md docs/research/feature-inventory.md
git commit -m "docs: record utility panel toggle"
```
