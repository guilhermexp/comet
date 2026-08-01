# Lateral Terminal Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the bottom terminal dock with a discoverable, session-scoped terminal in the resizable right-side card beside the chat.

**Architecture:** Model the utility area as one optional `UtilityPane` per session, so Terminal and Changes are mutually exclusive by construction. Reuse the existing right-pane width, animation, card, and resize handle; render either `TerminalPanel` or `Changes` inside it. Keep all PTY/RPC behavior unchanged.

**Tech Stack:** Rust 2024, GPUI, Tokio, Serde, existing Comet RPC and terminal emulator.

## Global Constraints

- Preserve `Cmd+J` for Terminal and `Cmd+B` for Changes.
- Keep the terminal icon visible whenever a space is selected, including the new-session canvas.
- Opening one utility pane replaces the other; clicking the active control closes it.
- Preserve terminal processes and tabs across close/open and chat switches.
- Reuse the persisted `right_pane_width` range: 360–760 px, default 520 px.
- Remove the obsolete bottom-dock height setting and vertical resize code.
- Add no dependency and change no backend or RPC contract.

---

### Task 1: Exclusive per-session utility-pane state

**Files:**
- Modify: `crates/ui/src/shell.rs:176-210`
- Test: `crates/ui/src/shell.rs:3933-3977`

**Interfaces:**
- Produces: `UtilityPane::{Terminal, Changes}`.
- Produces: `SessionPanels::active(&self, key: &str) -> Option<UtilityPane>`.
- Produces: `SessionPanels::toggle(&mut self, key: &str, pane: UtilityPane) -> bool`, returning whether `pane` is active after the operation.
- Consumes: stable session key from `Shell::panel_key`.

- [ ] **Step 1: Replace the existing panel-state tests with failing exclusive-state tests**

```rust
#[test]
fn utility_panes_default_closed_per_chat() {
    let panels = SessionPanels::default();
    assert_eq!(panels.active("a"), None);
    assert_eq!(panels.active("b"), None);
}

#[test]
fn utility_pane_toggle_is_exclusive_and_chat_scoped() {
    let mut panels = SessionPanels::default();

    assert!(panels.toggle("a", UtilityPane::Terminal));
    assert_eq!(panels.active("a"), Some(UtilityPane::Terminal));
    assert_eq!(panels.active("b"), None);

    assert!(panels.toggle("a", UtilityPane::Changes));
    assert_eq!(panels.active("a"), Some(UtilityPane::Changes));

    assert!(!panels.toggle("a", UtilityPane::Changes));
    assert_eq!(panels.active("a"), None);
}
```

- [ ] **Step 2: Run the focused tests and verify failure**

Run:

```bash
cargo test -p comet-ui utility_pane -- --nocapture
```

Expected: compilation fails because `UtilityPane`, `active`, and `toggle` do not exist.

- [ ] **Step 3: Implement the exclusive state model**

Replace `ChatPanels` and its two toggle methods with:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UtilityPane {
    Terminal,
    Changes,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ChatPanels {
    active: Option<UtilityPane>,
}

#[derive(Debug, Default)]
pub struct SessionPanels {
    map: std::collections::HashMap<String, ChatPanels>,
}

impl SessionPanels {
    pub fn active(&self, key: &str) -> Option<UtilityPane> {
        self.map.get(key).and_then(|panels| panels.active)
    }

    pub fn toggle(&mut self, key: &str, pane: UtilityPane) -> bool {
        let entry = self.map.entry(key.to_string()).or_default();
        entry.active = (entry.active != Some(pane)).then_some(pane);
        entry.active == Some(pane)
    }
}
```

- [ ] **Step 4: Run the focused tests and verify pass**

Run:

```bash
cargo test -p comet-ui utility_pane -- --nocapture
```

Expected: both utility-pane tests pass.

- [ ] **Step 5: Commit the state model**

```bash
git add crates/ui/src/shell.rs
git commit -m "refactor(ui): model one utility pane per session"
```

---

### Task 2: Move Terminal into the right-side card

**Files:**
- Modify: `crates/ui/src/shell.rs:38-46, 340-345, 415-540, 625-690, 790-818, 877-999, 2755-2936, 3010-3065, 3668-3686, 3721-3799`

**Interfaces:**
- Consumes: `UtilityPane` and `SessionPanels::{active,toggle}` from Task 1.
- Produces: `Shell::active_utility_pane(&self, cx: &App) -> Option<UtilityPane>`.
- Produces: one shared right-pane renderer for Terminal and Changes.
- Preserves: `Shell::toggle_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>)` and `Shell::toggle_right_pane(&mut self, cx: &mut Context<Self>)` call signatures.

- [ ] **Step 1: Update chat-switch restoration to expose compilation failures**

Use the new state contract in the chat-switch block:

```rust
self.right_tween = None;
let active = self.active_utility_pane(cx);
if let Some(panel) = self.terminal.clone() {
    panel.update(cx, |panel, cx| {
        panel.set_open(active == Some(UtilityPane::Terminal), cx)
    });
}
if active == Some(UtilityPane::Changes) {
    let changes = self.changes_pane(cx);
    changes.update(cx, |changes, cx| changes.ensure_watch(cx));
}
```

Run:

```bash
cargo check -p comet-ui
```

Expected: compilation fails at the remaining `terminal_open`, `changes_open`, `toggle_terminal`, and `toggle_changes` call sites.

- [ ] **Step 2: Route both toggles through the shared right pane**

Implement pane selection and visibility:

```rust
fn active_utility_pane(&self, cx: &App) -> Option<UtilityPane> {
    match self.panels.active(&self.panel_key(cx)) {
        Some(UtilityPane::Changes) if !self.space_git_detected(cx) => None,
        active => active,
    }
}

fn right_pane_open(&self, cx: &App) -> bool {
    self.active_utility_pane(cx).is_some()
}
```

Change the Changes toggle to:

```rust
fn toggle_right_pane(&mut self, cx: &mut Context<Self>) {
    if !self.space_git_detected(cx) {
        return;
    }
    let from = self.right_target(cx);
    let key = self.panel_key(cx);
    let open = self.panels.toggle(&key, UtilityPane::Changes);
    self.right_tween = Some(WidthTween::new(from, self.right_target(cx)));
    if let Some(terminal) = self.terminal.clone() {
        terminal.update(cx, |panel, cx| panel.set_open(false, cx));
    }
    if open {
        let changes = self.changes_pane(cx);
        changes.update(cx, |changes, cx| changes.ensure_watch(cx));
    }
    cx.notify();
}
```

Change the Terminal toggle to:

```rust
fn toggle_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    let from = self.right_target(cx);
    let key = self.panel_key(cx);
    let open = self.panels.toggle(&key, UtilityPane::Terminal);
    self.right_tween = Some(WidthTween::new(from, self.right_target(cx)));
    let panel = self.terminal_panel(cx);
    panel.update(cx, |panel, cx| panel.set_open(open, cx));
    if open {
        window.focus(&panel.read(cx).focus_handle(), cx);
    } else {
        window.focus(&self.composer.focus_handle(cx), cx);
    }
    cx.notify();
}
```

- [ ] **Step 3: Render the selected utility inside the existing right-side card**

Replace the Changes-only content selection with:

```rust
let content: AnyElement = match self.active_utility_pane(cx) {
    Some(UtilityPane::Terminal) => {
        let terminal = self.terminal_panel(cx);
        terminal.update(cx, |panel, cx| panel.set_open(true, cx));
        terminal.into_any_element()
    }
    Some(UtilityPane::Changes) => {
        let changes = self.changes_pane(cx);
        changes.update(cx, |changes, cx| changes.ensure_watch(cx));
        changes.into_any_element()
    }
    None => gpui::Empty.into_any_element(),
};
```

Keep the current card, horizontal resize handle, width tween, radius, border, and gutters unchanged.

- [ ] **Step 4: Remove the bottom-dock implementation**

Delete:

- `TerminalResize`.
- `terminal_tween`, `terminal_tween_task`, and `terminal_drag_anchor` fields and initializers.
- `terminal_target` and `on_terminal_drag`.
- `render_terminal_container`.
- `.child(self.render_terminal_container(cx))` from the conversation column.
- `.on_drag_move(cx.listener(Self::on_terminal_drag))` from the shell root.
- `TERMINAL_DEFAULT_HEIGHT` and `clamp_terminal_height` imports from `shell.rs`.

Do not remove `TerminalPanel`, `ToggleTerminal`, its focus behavior, or its close action.

- [ ] **Step 5: Run UI tests and compile the app**

Run:

```bash
cargo test -p comet-ui utility_pane -- --nocapture
cargo check -p comet
```

Expected: utility-pane tests pass and both packages compile without references to the bottom dock.

- [ ] **Step 6: Commit the lateral container migration**

```bash
git add crates/ui/src/shell.rs
git commit -m "feat(ui): move terminal into right utility pane"
```

---

### Task 3: Add active Terminal and Changes title-bar controls

**Files:**
- Modify: `crates/ui/src/shell.rs:3567-3603`
- Modify: `crates/ui/src/shell/tabs.rs:180-181, 226-232, 549-575`

**Interfaces:**
- Consumes: `Shell::active_utility_pane`, `Shell::toggle_terminal`, and `Shell::toggle_right_pane`.
- Produces: `header_icon_button(id, icon_path, active, theme, on_click)` with active styling.

- [ ] **Step 1: Add the active argument to the shared title-bar button**

Change its signature and background selection:

```rust
fn header_icon_button(
    id: &'static str,
    icon_path: &'static str,
    active: bool,
    theme: &Theme,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let muted = theme.text_muted;
    let fade_key = format!("header-icon-{id}");
    let bg = if active {
        crate::theme::glass_selected_bg()
    } else {
        motion::hover_blend(
            &fade_key,
            crate::theme::wash(0.0),
            crate::theme::wash(0.11),
        )
    };

    div()
        .id(id)
        .size(px(28.0))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(6.0))
        .cursor_pointer()
        .bg(bg)
        .when(active, |el| el.shadow(crate::theme::glass_selected_shadows()))
        .on_hover(motion::hover_listener(fade_key))
        .occlude()
        .on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
        .on_click(move |event, window, cx| {
            cx.stop_propagation();
            on_click(event, window, cx)
        })
        .child(icon(icon_path).size(px(16.0)).text_color(muted))
}
```

Update every call site to pass an active boolean.

- [ ] **Step 2: Add the Terminal control before Changes**

Compute the active state near the existing `git` variable:

```rust
let active_utility = self.active_utility_pane(cx);
let terminal_active = active_utility == Some(UtilityPane::Terminal);
let changes_active = active_utility == Some(UtilityPane::Changes);
```

Render the controls after the title-bar spacer:

```rust
.when(has_space, |el| {
    el.child(header_icon_button(
        "toggle-terminal",
        icons::TERMINAL,
        terminal_active,
        &theme,
        cx.listener(|this, _, window, cx| this.toggle_terminal(window, cx)),
    ))
})
.when(git, |el| {
    el.child(header_icon_button(
        "toggle-changes",
        icons::SIDEBAR_MINIMALISTIC,
        changes_active,
        &theme,
        cx.listener(|this, _, _, cx| this.toggle_right_pane(cx)),
    ))
})
```

- [ ] **Step 3: Compile the complete title bar**

Run:

```bash
cargo check -p comet
```

Expected: success; both controls use the new helper signature.

- [ ] **Step 4: Commit the discoverable controls**

```bash
git add crates/ui/src/shell.rs crates/ui/src/shell/tabs.rs
git commit -m "feat(ui): add lateral terminal titlebar control"
```

---

### Task 4: Remove obsolete bottom-terminal settings

**Files:**
- Modify: `crates/ui/src/settings.rs:1-93, 262-283, 324-393`
- Test: `crates/ui/src/settings.rs:324-393`

**Interfaces:**
- Preserves: deserialization of existing `ui-settings.json` files containing `terminalHeight` and `terminalOpen`; Serde ignores these unknown legacy fields.
- Removes: `TERMINAL_MIN_HEIGHT`, `TERMINAL_MAX_VH`, `TERMINAL_ABS_MAX_HEIGHT`, `TERMINAL_DEFAULT_HEIGHT`, `UiSettings::terminal_height`, and `UiSettings::terminal_open`.

- [ ] **Step 1: Add a failing legacy-settings migration test**

```rust
#[test]
fn legacy_terminal_layout_fields_are_not_reserialized() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        UiSettings::path(dir.path()),
        r#"{"terminalHeight":420,"terminalOpen":true,"rightPaneWidth":640}"#,
    )
    .unwrap();

    let loaded = UiSettings::load(dir.path());
    assert_eq!(loaded.right_pane_width, 640.0);
    loaded.save(dir.path()).unwrap();

    let saved = std::fs::read_to_string(UiSettings::path(dir.path())).unwrap();
    assert!(!saved.contains("terminalHeight"));
    assert!(!saved.contains("terminalOpen"));
}
```

- [ ] **Step 2: Run the test and verify failure**

Run:

```bash
cargo test -p comet-ui legacy_terminal_layout_fields_are_not_reserialized -- --nocapture
```

Expected: FAIL because current serialization still writes both legacy fields.

- [ ] **Step 3: Remove bottom-terminal settings and update existing tests**

Remove the terminal height constants, fields, defaults, and clamp block. Remove `terminal_height` and `terminal_open` from the `round_trip` fixture. Change the defaults assertion to:

```rust
assert!(!d.sidebar_collapsed && !d.right_pane_open);
```

Keep `right_pane_open` as the existing compatibility field because it predates this change and is outside the requested cleanup.

- [ ] **Step 4: Run settings and UI tests**

Run:

```bash
cargo test -p comet-ui settings::tests -- --nocapture
cargo test -p comet-ui
```

Expected: all settings tests and the full `comet-ui` suite pass.

- [ ] **Step 5: Commit settings cleanup**

```bash
git add crates/ui/src/settings.rs
git commit -m "refactor(ui): remove bottom terminal layout settings"
```

---

### Task 5: Runtime verification

**Files:**
- No source changes expected.

**Interfaces:**
- Verifies the complete user-visible contract from the approved design.

- [ ] **Step 1: Format and build the desktop binary**

Run:

```bash
cargo fmt --check
cargo build -p comet
```

Expected: formatting check and build succeed.

- [ ] **Step 2: Start the development demo with the new binary**

Run the existing `scripts/dev-demo.sh` through zsh because macOS Bash 3.2 cannot parse the script's `declare -A` usage. Keep `COMET_WORKOS_CLIENT_ID` empty for local dev auth.

Expected log milestones:

```text
starting engine daemon on :27921
opening comet
```

- [ ] **Step 3: Verify the lateral terminal interaction**

Using a selected existing chat:

1. Confirm a terminal glyph appears immediately left of Changes.
2. Click it and confirm a 520 px right-side card appears while the chat remains visible.
3. Confirm the terminal receives focus and accepts `printf 'lateral-terminal-ok\\n'`.
4. Drag the card's left edge and confirm horizontal resize stays within 360–760 px.
5. Open Changes and confirm it replaces Terminal in the same card.
6. Press `Cmd+J` and confirm Terminal replaces Changes.
7. Switch chats and back; confirm terminal tabs and PTY output are restored.
8. Select the new-session canvas and confirm the button remains visible and the panel says `Select a chat to open a terminal`.
9. Close the terminal with its `×` and confirm the chat expands to the window gutter.

Expected: every interaction matches the approved design; no second bottom terminal appears.

- [ ] **Step 4: Run focused RPC smoke check**

Run:

```bash
cargo run -q -p comet-rpc --example rpc_probe -- ws://127.0.0.1:27921 LocalDevice '{}'
```

Expected: one JSON object with a non-empty `deviceId`, proving the UI migration did not break the engine connection.
