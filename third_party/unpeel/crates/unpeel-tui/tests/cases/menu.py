"""The sidebar footer menu (herdr-style): a "menu" label on the bottom edge
opens a small popup with Settings, keybindings, and the command palette."""

import sys, os

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from harness import run  # noqa: E402


def _click(col, row):
    return f"\x1b[<0;{col};{row}M\x1b[<0;{col};{row}m".encode()


def body(case):
    home = case.home
    home.project("p", "unpeel", "/tmp")
    home.session("s1", label="alpha", project_id="p")

    tui = case.pty()  # 45 rows × 150 cols
    tui.read_for(3.0)
    case.check("the menu label sits on the sidebar footer", "menu" in tui.sidebar(), tui.sidebar()[-120:])

    # Click "menu" on the bottom border (0-based row 44 → SGR 45; label at
    # 0-based col 2 → SGR 3).
    tui.send(_click(3, 45), settle=0.8)
    popup = tui.expect("Command Palette")
    case.check(
        "clicking menu opens the popup",
        all(label in popup for label in ("Settings", "Keybindings", "Command Palette")),
        popup[:240],
    )
    menu_grid = tui.grid()
    shortcut_ends = []
    for label, shortcut in (
        ("Settings", ","),
        ("Keybindings", "?"),
        ("Command Palette", "ctrl+k"),
    ):
        row = next((line for line in menu_grid.lines() if label in line), "")
        case.check(
            f"{label} shows its shortcut",
            shortcut in row,
            repr(row),
        )
        shortcut_ends.append(row.find(shortcut) + len(shortcut))
    case.check(
        "menu shortcuts share a right-aligned column",
        len(set(shortcut_ends)) == 1,
        repr(shortcut_ends),
    )

    # Enter follows the first row → Settings.
    tui.send("\r", settle=1.0)
    settings = tui.expect("Presets")
    case.check("Settings opens the preferences panel", "Presets" in settings, settings[:160])
    tui.send("\x1b", settle=0.6)  # close settings, back to the sidebar

    # Reopen, arrow down to Keybindings, Enter → the help overlay.
    tui.send(_click(3, 45), settle=0.8)
    tui.expect("Keybindings")
    tui.send("\x1b[B", settle=0.4)  # down
    tui.send("\r", settle=0.8)
    help_text = tui.expect("navigation")
    case.check("Keybindings opens the help overlay", "navigation" in help_text, help_text[:160])

    # Esc/any key closes help; clicking off the menu also dismisses it.
    tui.send(" ", settle=0.5)

    # The added row reuses the real command-palette action. Its displayed
    # ctrl+K equivalent also stays live while the popup is open.
    tui.send(_click(3, 45), settle=0.6)
    tui.expect("Command Palette")
    tui.send(b"\x0b", settle=0.8)  # ctrl+K
    case.check(
        "the menu shortcut opens the command palette",
        "command palette" in tui.expect("command palette"),
    )
    tui.send("\x1b", settle=0.5)

    # The third row is clickable/navigable too, independent of the shortcut.
    tui.send(_click(3, 45), settle=0.6)
    tui.expect("Command Palette")
    tui.send("\x1b[B\x1b[B", settle=0.4)  # down twice
    tui.send("\r", settle=0.8)
    case.check(
        "Command Palette opens from its menu row",
        "command palette" in tui.expect("command palette"),
    )
    tui.send("\x1b", settle=0.5)

    tui.send(_click(3, 45), settle=0.6)
    tui.expect("Command Palette")
    tui.send(_click(60, 20), settle=0.6)  # click far away in the preview
    case.check(
        "clicking outside dismisses the menu",
        "Command Palette" not in tui.grid().text(),
        tui.grid().text()[:160],
    )


run("menu", body)
