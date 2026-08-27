"""An empty project offers a way in, and the wheel scrolls without
hijacking the selection.

Both exist because of the same failure: a project with no sessions had no
selectable child, so `n` had nothing to aim at and the folder was a dead
end."""

import sys, os

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from harness import run, wait_running  # noqa: E402


def body(case):
    home = case.home
    home.project("full", "has-sessions", "/tmp")
    home.project("empty", "no-sessions", home.path("empty-project"))
    os.makedirs(home.path("empty-project"), exist_ok=True)
    home.preset(label="cat", command="cat")
    home.session("s1", label="a session", project_id="full")

    tui = case.pty()
    tui.read_for(3.0)
    sidebar = tui.sidebar()

    case.check("the empty project still renders", "no-sessions" in sidebar, sidebar[:200])
    case.check("it offers a way in", "+ New session" in sidebar, sidebar[:200])
    case.check(
        "only the empty project gets the row",
        sidebar.count("+ New session") == 1,
        sidebar[:240],
    )
    case.check(
        "it sits under the empty project's header",
        sidebar.index("no-sessions") < sidebar.index("+ New session"),
        sidebar[:240],
    )
    # The full project drops the row entirely — the hover "+ New" on the
    # header is the way into a new session there, not a key hint.
    grid = tui.grid()
    case.check(
        "the full project's header carries no New hint",
        not any("New (n)" in line for line in grid.lines()),
        sidebar[:240],
    )

    # A collapsed project hides its row along with its sessions.
    tui.send("-", settle=0.8)
    case.check(
        "collapsing hides the row too",
        "+ New session" not in tui.sidebar(),
        tui.sidebar()[:200],
    )
    tui.send("-", settle=0.8)

    # Arrow down onto the row: highlight only, nothing should happen yet.
    tui.send("j", settle=0.8)
    case.check(
        "the row is selectable",
        "+ New session" in tui.sidebar(),
    )
    case.check("highlighting spawns nothing", home.running_sessions() == {})
    # The row is a button: its highlight spans the sidebar like a session's,
    # rather than stopping at the end of the words.
    grid = tui.grid()
    width = grid.sidebar_width()
    row = next(
        (line for line in grid.lines() if "+ New session" in line),
        "",
    )
    case.check(
        "the row fills the sidebar width",
        len(row[:width].rstrip()) >= width - 2,
        repr(row[:width]),
    )

    # Clicking it acts — it reads as a button, so highlight-only would
    # look broken (it did: "nothing happens if I click + New session").
    grid = tui.grid()
    new_row = next(
        (row for row in range(grid.rows) if "+ New session" in grid.row(row)),
        None,
    )
    case.check("the row is on screen to click", new_row is not None)
    tui.click(6, new_row)
    clicked = tui.expect("cat", timeout=8)
    case.check("clicking the row opens the picker", "cat" in clicked, clicked[:200])
    tui.send("\x1b", settle=0.8)

    # ⏎ opens the picker, named for the project it will spawn into.
    tui.send("\r", settle=1.2)
    picker = tui.expect("cat")
    case.check("⏎ opens the preset picker", "cat" in picker, picker[:200])
    case.check(
        "the picker names the destination project",
        "no-sessions" in picker,
        picker[:240],
    )

    # The open picker owns the mouse. A click outside is Esc…
    tui.click(2, 2)
    after = tui.screen()
    case.check(
        "clicking outside dismisses the picker",
        "pick a preset" not in after,
        after[:200],
    )
    case.check("dismissing spawns nothing", home.running_sessions() == {})

    # …and a click on a row launches THAT row, not the highlighted one
    # (selection sits on "Terminal"; we click the cat preset below it).
    tui.send("\r", settle=1.2)
    grid = tui.grid()
    cat_at = next(
        (
            (row, grid.row(row).find("cat"))
            for row in range(grid.rows)
            if "pick a preset" not in grid.row(row) and grid.row(row).find("cat") >= 40
        ),
        None,
    )
    case.check("the picker shows the cat preset row", cat_at is not None)
    if cat_at:
        tui.click(cat_at[1], cat_at[0])
    spawned = tui.wait_for(lambda: home.running_sessions(), timeout=25)
    case.check("clicking a preset spawns a session", bool(spawned), str(home.manifests().keys()))

    if spawned:
        session_id = next(iter(spawned))
        wait_running(home, session_id)
        manifest = home.manifests()[session_id]
        case.check(
            "the session lands in the empty project",
            manifest["session"]["project_id"] == "empty",
            str(manifest["session"]["project_id"]),
        )
        case.check(
            "and in that project's directory",
            manifest["cwd"] == home.path("empty-project"),
            str(manifest["cwd"]),
        )
        case.check(
            "the clicked row is the one launched",
            manifest["session"]["command"] == "cat",
            str(manifest["session"].get("command")),
        )
        # The project is no longer empty, so its row folds away — the
        # hover "+ New" on the header is the way to a second session.
        after = tui.expect(absent=("+ New session",), timeout=8)
        case.check(
            "the row folds away once a session exists",
            "+ New session" not in after,
            after[:240],
        )
        # The point of starting one is to work in it: the new session takes
        # the selection rather than leaving it on the row that spawned it.
        # Before this spawn the preview showed the OTHER project's stopped
        # session; now it must show the live one we just started, not the
        # row that started it.
        live = tui.expect(absent=("is stopped",), timeout=12)
        case.check(
            "the new session takes the selection",
            "is stopped" not in live,
            live[:200],
        )

    # The picker's footer row is a shortcut into Settings ▸ Presets — the
    # place to fix the list when the preset you wanted isn't there.
    tui.send("n", settle=1.2)
    picker = tui.expect("manage presets")
    case.check("the picker offers a manage-presets row", "manage presets" in picker, picker[:240])
    tui.send("jj", settle=0.6)  # Terminal → cat → manage presets (last row)
    tui.send("\r", settle=1.2)
    settings = tui.expect("Presets")
    case.check(
        "⏎ on it opens Settings ▸ Presets",
        "Presets" in settings and "pick a preset" not in settings,
        settings[:240],
    )
    case.check("the shortcut spawns nothing new", len(home.running_sessions()) <= 1)
    tui.send("\x1b", settle=0.8)


run("empty_project", body)
