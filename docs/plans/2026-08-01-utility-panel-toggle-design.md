# Utility Panel Toggle Design

**Date:** 2026-08-01  
**Status:** Approved

## Goal

Replace the external Changes toggle with one control for the entire right utility panel. Keep open utility tabs intact while the panel is hidden, restore them in one click, and offer a Terminal/Changes chooser when no utility tab exists.

## Approved behavior

- Keep the external Terminal button as a direct Terminal shortcut.
- Replace the external Changes button with the utility-panel toggle.
- Keep the current split-panel glyph for the utility-panel toggle.
- Give Changes a new, distinct diff glyph in the shared tab strip and both add menus.
- When the utility panel is visible, the new control hides the whole column without closing Terminal sessions or Changes.
- When the utility panel is hidden and open utility tabs exist, one click restores every existing tab and selects the previously active tab.
- When the utility panel is hidden and no utility tab exists, the control opens a menu anchored to itself with Terminal and Changes choices.
- `Cmd+B` continues to open Changes directly. `Cmd+J` continues to open Terminal directly.

## State and data flow

`SessionPanels` remains the per-session source of truth for visibility, active utility tab, and whether Changes exists. Terminal existence remains derived from `TerminalPanel::has_tabs`.

The panel toggle evaluates the selected session in this order:

1. Visible panel: hide it while preserving tab state.
2. Hidden panel with available tabs: restore visibility and the last valid active tab; if that tab no longer exists, fall back to an available Terminal tab and then Changes.
3. Hidden panel without tabs: open the external launcher menu without opening an empty panel.

The existing in-panel `+` menu and the new external launcher use the same Terminal and Changes actions.

## UI

- External controls: `Terminal` shortcut followed by the generic utility-panel toggle.
- The utility-panel toggle uses the existing `sidebar-minimalistic` asset and shows selected styling whenever the panel is visible.
- Changes uses a new `diff.svg` asset in its tab and menu rows.
- The external launcher menu matches the existing frosted add menu.

## Verification

- Unit tests: hide/restore with Terminal and Changes present; restore fallback after the last active tab closes; no-tabs launcher decision; state isolation across sessions.
- UI smoke: external toggle closes and restores `Terminal 1`, `Terminal 2`, and `Changes`; after closing every tab, the external toggle opens the Terminal/Changes menu.
- Project checks: `cargo test -p comet-ui`, `cargo build -p comet`, and local RPC probe.
