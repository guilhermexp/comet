# Sidebar Modes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add an Orchestrator/Workers segmented control above the sidebar, preserving all current content under Orchestrator and leaving Workers empty.

**Architecture:** Add a session-local `SidebarMode` enum to `Shell`. Render the selector at the sidebar container boundary and conditionally mount the existing route-specific sidebar content only for Orchestrator.

**Tech Stack:** Rust, GPUI, Cargo tests.

## Global Constraints

- Orchestrator is the default on every app launch.
- Existing sidebar content and behavior remain unchanged under Orchestrator.
- Workers renders no content below the selector.
- The selection is session-local and is not added to `UiSettings`.
- No new dependency is introduced.

---

### Task 1: Sidebar mode selector

**Files:**
- Modify: `crates/ui/src/shell.rs`
- Test: `crates/ui/src/shell.rs`

**Interfaces:**
- Consumes: `Theme`, `Shell::render_sidebar`, and GPUI click listeners.
- Produces: `SidebarMode`, `Shell::sidebar_mode`, and stable IDs `sidebar-mode-orchestrator` / `sidebar-mode-workers`.

- [ ] **Step 1: Write the failing mode-state tests**

Add to `crates/ui/src/shell.rs` tests:

```rust
#[test]
fn sidebar_mode_defaults_to_orchestrator() {
    assert_eq!(SidebarMode::default(), SidebarMode::Orchestrator);
}

#[test]
fn workers_mode_hides_orchestrator_content() {
    assert!(SidebarMode::Orchestrator.shows_orchestrator_content());
    assert!(!SidebarMode::Workers.shows_orchestrator_content());
}
```

- [ ] **Step 2: Run the tests to verify RED**

Run:

```bash
cargo test -p zeron-ui sidebar_mode
```

Expected: compilation fails because `SidebarMode` does not exist.

- [ ] **Step 3: Add the minimal state model**

Add near `Route`:

```rust
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum SidebarMode {
    #[default]
    Orchestrator,
    Workers,
}

impl SidebarMode {
    fn shows_orchestrator_content(self) -> bool {
        matches!(self, Self::Orchestrator)
    }
}
```

Add `sidebar_mode: SidebarMode` to `Shell` and initialize it with
`SidebarMode::default()` in `Shell::new`.

- [ ] **Step 4: Run the state tests to verify GREEN**

Run:

```bash
cargo test -p zeron-ui sidebar_mode
```

Expected: both new tests pass.

- [ ] **Step 5: Render the selector and conditional content**

Add a `render_sidebar_mode_button` helper that renders an equal-width button,
uses the stronger fill/text treatment when its mode is selected, and updates
`self.sidebar_mode` from its click listener. Add
`render_sidebar_mode_switcher` to render both buttons in a rounded inset
container using the exact labels `Orchestrator` and `Workers`.

Update `render_sidebar` to this composition:

```rust
let inner = self.sidebar_mode.shows_orchestrator_content().then(|| match self.route {
    Route::Settings(section) => self.render_settings_nav(section, &theme, cx),
    Route::Chat => self.render_chat_sidebar(&theme, cx),
});

div()
    .h_full()
    .pt(px(Theme::TITLEBAR_HEIGHT))
    .flex()
    .flex_col()
    .child(self.render_sidebar_mode_switcher(&theme, cx))
    .when_some(inner, |el, content| {
        el.child(div().flex_1().min_h_0().child(content))
    })
```

The button click listener must set the selected mode and call `cx.notify()`.

- [ ] **Step 6: Run focused and crate validation**

Run:

```bash
cargo fmt --all --check
cargo test -p zeron-ui sidebar_mode
cargo test -p zeron-ui
```

Expected: formatting and all `zeron-ui` tests pass.

- [ ] **Step 7: Commit**

```bash
git add crates/ui/src/shell.rs
git commit -m "feat(ui): add sidebar mode selector"
```

- [ ] **Step 8: Restart and visually verify**

Run `cargo run -p zeron`. Confirm:

1. Orchestrator is selected initially.
2. Existing sidebar content is unchanged below it.
3. Workers shows only the selector.
4. Returning to Orchestrator restores the current sidebar content and state.
