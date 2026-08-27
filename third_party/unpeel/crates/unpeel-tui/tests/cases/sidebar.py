"""The sidebar: projects group sessions, selection moves, the preview
follows. This is the case that fails first if disk discovery breaks."""

import sys, os

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from harness import run, squeeze  # noqa: E402


def body(case):
    home = case.home
    home.project("p", "unpeel", "/tmp")
    home.project("q", "website", "/tmp")
    home.session("s-one", label="first session", project_id="p")
    home.session("s-two", label="second session", project_id="p")
    home.session("s-three", label="site session", project_id="q", command="codex")

    tui = case.pty()
    tui.read_for(3.0)
    screen = tui.screen()

    case.check("project headers render", "unpeel" in screen and "website" in screen)
    case.check("sessions listed under projects", "first session" in screen)
    case.check("second project's session listed", "site session" in screen)
    case.check("header says Projects", "Projects" in screen)

    # Selection starts on the first row; the preview names it.
    case.check("preview follows selection", "first session" in screen)

    tui.send("j")
    screen = tui.screen()
    case.check("j moves selection", "second session" in screen)

    # '-' collapses every project; pressing it again expands them.
    tui.send("-")
    collapsed = tui.sidebar()
    case.check(
        "collapse-all hides sessions",
        "first session" not in collapsed and "unpeel" in collapsed,
        collapsed[:160],
    )
    case.check("collapsed header shows ▸", "▸" in collapsed)
    tui.send("-")
    case.check("expand-all restores them", "first session" in tui.sidebar())

    # The same toggle lives as a clickable "-" on the sidebar's bottom-right
    # border; once everything is folded it reads "+" and expands again.
    grid = tui.grid()
    width = grid.sidebar_width()
    bottom = grid.rows - 1
    tui.click(width - 2, bottom)
    folded_all = tui.sidebar()
    case.check(
        "bottom-right - folds everything",
        "first session" not in folded_all and "unpeel" in folded_all,
        folded_all[:160],
    )
    case.check(
        "the folded toggle reads +",
        "+" in tui.grid().row(bottom)[:width],
        tui.grid().row(bottom)[:width],
    )
    tui.click(width - 2, bottom)
    case.check("clicking + expands everything", "first session" in tui.sidebar())

    # Clicking a project header collapses just that project.
    tui.click(5, 1)
    one = tui.sidebar()
    case.check(
        "header click collapses one project",
        "first session" not in one and "site session" in one,
        one[:160],
    )
    tui.click(5, 1)
    tui.read_for(0.8)

    # The header's right edge is its "+ New" action (painted on hover): clicking
    # there opens the new-session picker for THAT project instead of
    # collapsing it — the mouse way in now that a project with sessions
    # has no "+ New session" row.
    width = tui.grid().sidebar_width()
    tui.click(width - 3, 1)
    picker = tui.expect("pick a preset")
    case.check(
        "the header's + New opens the preset picker",
        "pick a preset" in picker,
        picker[:200],
    )
    case.check(
        "named for the header's own project",
        "unpeel · /tmp" in picker,
        picker[:240],
    )
    grid = tui.grid()
    case.check(
        "the header's + opens a dropdown directly below it",
        "pick a preset" in grid.row(2),
        "\n".join(grid.lines()[:8]),
    )
    tui.send("\x1b", settle=0.8)
    case.check(
        "dismissing leaves the project uncollapsed",
        "first session" in tui.sidebar(),
    )

    # The keyboard path intentionally keeps the roomier centered dialog.
    tui.send("n", settle=0.8)
    grid = tui.grid()
    title_row = next(
        (row for row in range(grid.rows) if "pick a preset" in grid.row(row)),
        None,
    )
    case.check(
        "n keeps the centered new-session dialog",
        title_row is not None and title_row > 2,
        "\n".join(grid.lines()[:12]),
    )
    tui.send("\x1b", settle=0.8)

    # Right-click a project header: its context menu (the desktop's,
    # trimmed to what exists here), anchored at the pointer.
    tui.click(5, 1, button=2)
    menu = tui.expect("New session")
    case.check("right-click opens the project menu", "New session" in menu, menu[:240])
    case.check("collapse is offered", "Collapse" in menu, menu[:240])
    tui.send("\x1b", settle=0.6)
    case.check("esc dismisses the menu", "New session" not in tui.screen())

    # Its Collapse row folds the project, like clicking the header does.
    tui.click(5, 1, button=2)
    tui.expect("Collapse")
    # Two rows down: New session, New group… → Collapse.
    tui.send("jj", settle=0.4)
    tui.send("\r", settle=0.8)
    folded = tui.sidebar()
    case.check(
        "the menu's collapse folds the project",
        "first session" not in folded and "site session" in folded,
        folded[:200],
    )
    tui.click(5, 1, button=2)
    expanded_menu = tui.expect("Expand")
    case.check("a folded project offers expand", "Expand" in expanded_menu, expanded_menu[:240])
    tui.send("jj", settle=0.4)
    tui.send("\r", settle=0.8)
    case.check("and expand restores it", "first session" in tui.sidebar())

    # Right-click a session row: the desktop's session verbs, trimmed to
    # what exists here. These are stopped fixtures, so Resume and
    # remove-from-list are the offered shapes.
    grid = tui.grid()
    sess_row = next(r for r in range(grid.rows) if "first session" in grid.row(r))
    tui.click(5, sess_row, button=2)
    smenu = tui.expect("Rename")
    case.check(
        "right-click opens the session menu",
        "Rename" in smenu and "Resume" in smenu,
        smenu[:240],
    )
    case.check(
        "a stopped session offers remove-from-list",
        "Remove from list" in smenu,
        smenu[:240],
    )
    tui.send("\x1b", settle=0.6)

    # A session that vanishes from disk leaves the sidebar.
    import shutil

    shutil.rmtree(home.path("app-sessions", "s-three"))
    gone = tui.wait_for(lambda: "site session" not in squeeze(tui.frame(0.6)), timeout=15)
    case.check("removed session disappears", gone)

    # Right-click ▸ Remove project…: the rows swap for an inline confirm
    # (desktop's row-swap), then the record leaves the shared file. Hosts
    # are never killed — removing a project only forgets it.
    grid = tui.grid()
    head_row = next(r for r in range(grid.rows) if "website" in grid.row(r))
    tui.click(5, head_row, button=2)
    pmenu = tui.expect("Remove project")
    case.check("an owned project offers removal", "Remove project" in pmenu, pmenu[:240])
    # Five rows down: New session, New group…, Collapse, Sort sessions,
    # Reveal → Remove.
    tui.send("jjjjj", settle=0.6)
    tui.send("\r", settle=0.8)
    confirm = tui.expect("Cancel")
    case.check(
        "removal confirms inline",
        "Remove website?" in confirm and "Cancel" in confirm,
        confirm[:240],
    )
    tui.send("j", settle=0.4)
    tui.send("\r", settle=1.2)
    project_gone = tui.wait_for(lambda: "website" not in tui.sidebar(), timeout=10)
    case.check("the project leaves the sidebar", project_gone)
    case.check(
        "and the shared file",
        all(p.get("id") != "q" for p in home.state().get("projects", [])),
        str(home.state().get("projects")),
    )


run("sidebar", body)
