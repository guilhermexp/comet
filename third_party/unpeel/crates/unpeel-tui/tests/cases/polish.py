"""The details that make it feel like Unpeel rather than a table: help,
preset ordering, quick-launch stars, and a layout that survives a restart."""

import sys, os, json

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from harness import run  # noqa: E402


def body(case):
    home = case.home
    home.project("p", "unpeel", "/tmp")
    home.preset(label="alpha", command="alpha")
    home.preset(label="bravo", command="bravo")
    home.session("s1", label="a session", project_id="p")

    tui = case.pty()
    tui.read_for(3.0)

    # The resting UI has no bottom row at all: the single way in is the
    # "menu" on the sidebar's bottom-left border (settings, keybindings,
    # command palette) — every other key lives in the (?) overlay.
    grid = tui.grid()
    case.check(
        "the menu is the only bottom-border chrome",
        "menu" in grid.sidebar() and "(^K)" not in grid.sidebar(),
        grid.sidebar()[-120:],
    )
    case.check(
        "the panes reach the bottom — no status row of its own",
        "└" in grid.status() and "┘" in grid.status(),
        repr(grid.status()),
    )
    case.check(
        "the verb list is gone from the chrome",
        "n new" not in grid.text() and "q quit" not in grid.text(),
        grid.status(),
    )
    tui.send("?", settle=1.0)
    help_screen = tui.expect("command palette")
    case.check(
        "the help overlay documents the real keys",
        "command palette" in help_screen and "selection mode" in help_screen,
        help_screen[:240],
    )
    tui.send("\x1b", settle=0.8)

    tui.send(",", settle=1.2)
    presets = tui.expect("Presets")
    case.check(
        "presets explain what order means",
        "topmost enabled preset wins" in presets,
        presets[:240],
    )

    # J/K reorder; the order is the default-choosing mechanism.
    tui.send("J", settle=1.2)
    labels = [preset["label"] for preset in home.state()["presets"]]
    case.check("reordering persists", labels == ["bravo", "alpha"], str(labels))
    tui.send("K", settle=1.2)
    labels = [preset["label"] for preset in home.state()["presets"]]
    case.check("reordering back works too", labels == ["alpha", "bravo"], str(labels))

    tui.send("j", settle=0.5)
    tui.send("*", settle=1.2)
    case.check(
        "starring persists as quick_launch",
        home.state()["presets"][1]["quick_launch"] is True,
        str(home.state()["presets"][1]),
    )
    case.check("the star renders", "⭑" in tui.expect("⭑"))

    tui.send("\x1b", settle=0.8)

    # Layout (width + folded projects) survives a restart.
    # The divider sits on the sidebar's right border (0-based col 35 for
    # the default 36-wide sidebar).
    tui.drag((35, 9), (64, 9))
    tui.read_for(1.0)
    tui.send("-", settle=0.8)
    tui.send("q", settle=1.2)
    tui.exited()

    layout_path = home.path("tui-layout.json")
    case.check("layout is saved on quit", os.path.exists(layout_path))
    if os.path.exists(layout_path):
        with open(layout_path) as handle:
            saved = json.load(handle)
        case.check(
            "the dragged width is saved",
            saved.get("sidebar_width", 0) >= 60,
            str(saved),
        )
        case.check("folded projects are saved", len(saved.get("collapsed", [])) >= 1, str(saved))

    second = case.pty()
    second.read_for(3.0)
    case.check("the folded state is restored", "▸" in second.expect("▸"))


run("polish", body)
