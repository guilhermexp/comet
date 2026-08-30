# Tasks

## Fasing

| Fase | U-IDs | Seções | Depends on | Audit state | Audited commit | Entrega | UAT mode |
|---|---|---|---|---|---|---|---|
| F1 | I1-I5 | §1-§4 | — | pending | — | UI Dev Inspector end-to-end | human-driven |

## 1. Inspector module and renderer

**must_haves:** The inspector module compiles in debug assertions, registers the action with deferred toggle, and provides an inspector panel renderer.

- [x] I1 Implement `crates/ui/src/inspector.rs` with `ToggleInspector` action, `init(cx)`, and `render_inspector` panel renderer. files: `crates/ui/src/inspector.rs`. verify: `cargo test -p zeron-ui`.
- [x] I2 Wire `inspector::init(cx)` into application boot in `crates/ui/src/lib.rs` under `#[cfg(debug_assertions)]`. files: `crates/ui/src/lib.rs`. verify: `cargo check -p zeron`.

## 2. Keymap and menu registration

**must_haves:** `cmd-alt-i` shortcut is registered inside `apply_keymap` to survive boot keymap clearing, and menu item is present in debug mode.

- [x] I3 Bind `cmd-alt-i` in `crates/ui/src/shell.rs` within `apply_keymap` under `#[cfg(debug_assertions)]`. files: `crates/ui/src/shell.rs`. verify: `cargo test -p zeron-ui`.
- [x] I4 Add "Toggle GPUI Inspector" menu item in `crates/ui/src/app_menus.rs` under `#[cfg(debug_assertions)]`. files: `crates/ui/src/app_menus.rs`. verify: `cargo test -p zeron-ui`.

## 3. DOX and documentation

**must_haves:** Append-only update to `crates/ui/AGENTS.md` documenting the inspector capability and shortcut.

- [x] I5 Update `crates/ui/AGENTS.md` with inspector documentation. files: `crates/ui/AGENTS.md`. verify: `git diff crates/ui/AGENTS.md`.

## 4. Verification

**must_haves:** All workspace checks, release checks, UI tests, and visual screenshots prove correct behavior.

- [ ] Run cargo fmt and checks. files: `crates/ui`. verify: `cargo fmt --all && cargo build -p zeron && cargo check --release -p zeron && cargo test -p zeron-ui`.
- [ ] Run OpenSpec strict validation. files: `openspec/changes/add-ui-dev-inspector`. verify: `openspec validate --strict add-ui-dev-inspector`.
- [ ] Perform headed visual verification with `screencapture` output files saved. files: `~/.orchestrator/outputs/inspector-*.png`. verify: visual confirmation of inspector panel, highlight, and source location.
