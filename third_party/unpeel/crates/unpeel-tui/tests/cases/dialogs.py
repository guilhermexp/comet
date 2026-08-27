"""Rename and add-project use the same centred dialog; adding a preset is
the inline blank row at the bottom of Settings ▸ Presets — select it, type
the command, ⏎ commits, esc clears the draft."""

import sys, os

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from harness import run  # noqa: E402


def body(case):
    home = case.home
    home.project("p", "unpeel", "/tmp")
    home.preset(label="cat", command="cat")
    home.session("s1", label="rename me", project_id="p")

    tui = case.pty()
    tui.read_for(3.0)

    # ── rename ──
    tui.send("e", settle=0.8)
    dialog = tui.expect("rename session")
    case.check("rename opens a dialog", "rename session" in dialog)
    case.check("dialog offers save and cancel", "esc cancel" in dialog, dialog[:160])
    case.check("rename is prefilled with the title", "rename me" in dialog)

    tui.backspace(9)
    tui.type("renamed ok")
    tui.send("\r", settle=1.5)
    title = home.read_marker("s1", "title.json")
    case.check(
        "rename writes the shared title marker",
        bool(title) and title.get("title", "").startswith("renamed"),
        str(title),
    )
    case.check("the new title shows in the list", "renamed ok" in tui.expect("renamed ok"))

    # ── escape cancels without writing ──
    tui.send("e", settle=0.8)
    tui.type("throwaway")
    tui.send("\x1b", settle=0.8)
    case.check(
        "escape leaves the title alone",
        home.read_marker("s1", "title.json").get("title") == "renamed ok",
    )

    # ── new preset, typed into the inline add row inside settings ──
    tui.send(",", settle=1.2)
    settings = tui.expect("Add command")
    case.check("the preset list ends in a blank add row", "Add command" in settings)

    tui.send("+", settle=0.8)  # jumps selection to the add row
    tui.type("htop")
    case.check("the draft renders in the row", "htop" in tui.expect("htop"))
    tui.send("\r", settle=1.5)
    labels = [preset["label"] for preset in home.state()["presets"]]
    case.check("⏎ adds the preset", "htop" in labels, str(labels))

    # After a commit the selection follows the add row — a second draft
    # types straight away, and esc clears it without touching the list.
    tui.type("junk")
    tui.send("\x1b", settle=0.8)
    case.check(
        "escape adds nothing",
        [p["label"] for p in home.state()["presets"]] == labels,
    )

    # ── add project, also a dialog ──
    tui.send("\x1b", settle=0.8)
    tui.send("+", settle=1.0)
    project_dialog = tui.expect("project")
    case.check("add project opens a dialog", "project" in project_dialog)


run("dialogs", body)
