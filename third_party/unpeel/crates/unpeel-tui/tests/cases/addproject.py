"""Adding a project: a dialog rooted at the user's home, with tab
completion and a marker for git repos."""

import sys, os

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from harness import run  # noqa: E402


def body(case):
    home = case.home
    user_home = home.path("user-home")
    for name in ("Desktop", "Dev", "Documents", "Downloads"):
        os.makedirs(os.path.join(user_home, name), exist_ok=True)
    os.makedirs(os.path.join(user_home, "Dev", "unpeel", ".git"), exist_ok=True)
    home.project("p", "existing", "/tmp")
    home.session("s1", label="a session", project_id="p")

    tui = case.pty(cols=160, env={"HOME": user_home})
    tui.read_for(3.0)

    tui.send("+", settle=1.0)
    dialog = tui.expect("add a project")
    case.check("the dialog opens", "add a project" in dialog)
    case.check("it starts at the user's home", "~/" in dialog, dialog[:200])
    case.check(
        "home folders are listed",
        any(name in dialog for name in ("Dev", "Documents", "Desktop", "Downloads")),
        dialog[:240],
    )
    case.check("completion is explained", "tab completes" in dialog, dialog[:240])

    tui.type("Dev")
    tui.send("\t", settle=1.0)
    inside = tui.expect("Dev/")
    case.check("tab descends into a folder", "Dev/" in inside, inside[:200])
    case.check("git repos are marked", "git" in inside, inside[:240])

    # Complete to the unpeel repo and add it.
    tui.type("unpeel")
    tui.send("\t", settle=0.8)
    tui.send("\r", settle=1.5)
    names = [project["name"] for project in home.state().get("projects", [])]
    case.check("the project is added", any("unpeel" in name for name in names), str(names))
    case.check("the existing project is untouched", "existing" in names, str(names))

    # The same folder cannot become a second project — including when the
    # existing one is the app's, which lives in ITS defaults and not in
    # app-state.json (that is how a duplicate "unpeel" got created).
    before = len(home.state()["projects"])
    tui.send("+", settle=1.0)
    tui.type("Dev/unpeel")
    tui.send("\r", settle=1.5)
    case.check(
        "a folder already added is refused",
        len(home.state()["projects"]) == before,
        str([p["path"] for p in home.state()["projects"]]),
    )
    case.check(
        "and it takes you to the one you have",
        "already covers that folder" in tui.expect("already covers"),
        tui.grid().status(),
    )
    tui.send("\x1b", settle=0.6)

    # Escape adds nothing.
    before = home.state()["projects"]
    tui.send("+", settle=0.8)
    tui.type("Documents")
    tui.send("\x1b", settle=0.8)
    case.check("escape adds nothing", home.state()["projects"] == before)

    # ── mouse: click picks a row, [ add ] commits it, outside cancels ──
    tui.send("+", settle=1.0)
    tui.expect("add a project")
    grid = tui.grid()
    lines = grid.lines()
    title_row = next(i for i, l in enumerate(lines) if "add a project" in l)
    left = lines[title_row].index("add a project") - 2
    bottom_row = next(i for i, l in enumerate(lines) if "tab completes" in l)
    add_row = next((i for i, l in enumerate(lines) if "[ add ]" in l), None)
    case.check("the [ add ] chip sits on the frame", add_row == bottom_row)
    add_col = lines[add_row].index("[ add ]") + 3
    dev_row = next(
        i
        for i, l in enumerate(lines)
        if title_row < i < bottom_row and l[left + 1 :].strip().split(" ")[0] == "Dev"
    )
    tui.click(left + 4, dev_row)  # select the Dev row
    tui.click(add_col, add_row)  # commit it
    tui.read_for(1.0)
    names = [project["name"] for project in home.state().get("projects", [])]
    case.check("click + [ add ] adds the picked folder", "Dev" in names, str(names))

    # Double-click descends instead of adding.
    tui.send("+", settle=1.0)
    lines = tui.grid().lines()
    title_row = next(i for i, l in enumerate(lines) if "add a project" in l)
    bottom_row = next(i for i, l in enumerate(lines) if "tab completes" in l)
    left = lines[title_row].index("add a project") - 2
    docs_row = next(
        i
        for i, l in enumerate(lines)
        if title_row < i < bottom_row and l[left + 1 :].strip().split(" ")[0] == "Documents"
    )
    col, row = left + 4 + 1, docs_row + 1  # 1-based for raw SGR
    tui.send(
        f"\x1b[<0;{col};{row}M\x1b[<0;{col};{row}m"
        f"\x1b[<0;{col};{row}M\x1b[<0;{col};{row}m".encode(),
        settle=1.2,
    )
    descended = tui.expect("Documents/")
    case.check("double-click descends into the folder", "Documents/" in descended)

    # A click outside the dialog cancels it, like esc.
    projects_before = home.state()["projects"]
    tui.click(2, title_row)
    case.check(
        "a click outside cancels",
        "add a project" not in tui.expect_missing("add a project"),
    )
    case.check("and adds nothing", home.state()["projects"] == projects_before)


run("addproject", body)
