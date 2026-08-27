"""Double-clicking a session name opens the rename dialog — the mouse
equivalent of `e`, matching the desktop app. crossterm reports only single
presses, so the TUI times two clicks itself (see `App::last_click`)."""

import sys, os

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from harness import run  # noqa: E402


def _click(col, row):
    """One SGR press+release at 1-based screen cell (col,row)."""
    return f"\x1b[<0;{col};{row}M\x1b[<0;{col};{row}m".encode()


def sidebar_session_rows(tui):
    grid = tui.grid()
    width = grid.sidebar_width()
    rows = []
    for row, line in enumerate(grid.lines()):
        sidebar = line[:width]
        for name in ("alpha", "bravo"):
            if name in sidebar:
                rows.append((row, name))
    return sorted(rows)


def body(case):
    home = case.home
    home.project("p", "unpeel", "/tmp")
    home.session("s-a", label="alpha", project_id="p", created_at=1_754_100_000_000)
    home.session("s-b", label="bravo", project_id="p", created_at=1_754_200_000_000)

    tui = case.pty()
    tui.read_for(3.5)
    screen = tui.sidebar()
    case.check("sessions render", "bravo" in screen and "alpha" in screen, screen[:200])
    rows = sidebar_session_rows(tui)
    first_row, first_name = rows[0]
    second_row, _second_name = rows[1]

    # A single click selects but must NOT open rename. Target the rendered
    # row: exited lifecycle time, not the old fixture creation stamp, owns
    # ordering.
    tui.send(_click(5, first_row + 1), settle=0.8)
    case.check(
        "a single click does not open rename",
        "rename session" not in tui.grid().text(),
        tui.grid().text()[:200],
    )

    # Two rapid clicks on the same row — inside the 400ms window — do.
    tui.send(_click(5, first_row + 1) + _click(5, first_row + 1), settle=1.0)
    dialog = tui.expect("rename session")
    case.check("double-click opens the rename dialog", "rename session" in dialog, dialog[:200])
    case.check(
        "it is seeded with the session's name",
        first_name in dialog,
        dialog[:200],
    )

    # Switching to another rendered session closes the dialog — a half-typed
    # name for the session you left is never what you want.
    tui.send(_click(5, second_row + 1), settle=0.8)
    case.check(
        "changing session closes the rename dialog",
        "rename session" not in tui.grid().text(),
        tui.grid().text()[:200],
    )

    # Re-open the first row and rename it for real, through the same path as
    # `e`.
    tui.send(_click(5, first_row + 1) + _click(5, first_row + 1), settle=1.0)
    tui.expect("rename session")
    # And it actually renames through the same path as `e`.
    tui.type("-x")
    tui.send("\r", settle=1.5)
    renamed = f"{first_name}-x"
    titles = tui.expect(renamed)
    case.check("the rename saves", renamed in titles, titles[:200])


run("dblclick", body)
