"""Ctrl+K is the whole command palette, not a recents list: sessions,
projects, presets and commands in one search — and it opens even while a
session holds the keyboard."""

import sys, os

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from harness import run  # noqa: E402


def body(case):
    home = case.home
    home.project("p", "unpeel", "/tmp")
    home.preset(label="cat", command="cat")
    home.session("s1", label="first session", project_id="p")
    home.session("s2", label="second session", project_id="p")
    home.session("s-arch", label="zebra archived thing", project_id="p")
    home.marker("s-arch", "archived.json", {"archived_at": 1})

    tui = case.pty()
    tui.read_for(3.0)

    tui.send(b"\x0b", settle=1.0)  # ctrl+K
    palette = tui.expect("command palette")
    case.check("ctrl+K opens the command palette", "command palette" in palette)
    case.check(
        "it mixes every kind of row",
        "Session" in palette and "Command" in palette,
        palette[:240],
    )

    tui.type("new session")
    launches = tui.expect("New session:")
    case.check("preset launches are offered", "New session:" in launches)

    tui.backspace(11)
    tui.type("settings")
    commands = tui.expect("Settings")
    case.check("fuzzy search finds commands", "Settings" in commands)

    tui.send("\r", settle=1.5)
    case.check("selecting a command runs it", "Presets" in tui.expect("Presets"))
    tui.send("\x1b", settle=0.8)

    # Archived sessions surface only when nothing active matches.
    tui.send(b"\x0b", settle=0.8)
    tui.type("zebra")
    archived = tui.expect("zebra archived thing")
    case.check(
        "archived sessions surface when nothing else matches",
        "zebra archived thing" in archived,
        archived[:200],
    )
    tui.send("\x1b", settle=0.6)

    # The palette must be reachable while a session owns the keyboard: the
    # plain-key shortcuts are gone then, so ctrl+K is the only way through.
    tui.send("\r", settle=1.2)  # focus the session
    tui.send(b"\x0b", settle=1.0)
    focused = tui.expect("command palette")
    case.check(
        "ctrl+K still opens with a session focused",
        "command palette" in focused,
        focused[:200],
    )
    tui.send("\x1b", settle=0.6)


run("palette", body)
