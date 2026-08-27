"""Long rename values wrap, and a mouse drag selects editable text."""

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from harness import run  # noqa: E402


def body(case):
    home = case.home
    home.project("p", "unpeel", "/tmp")
    title = "prefix DELETE-ME suffix " + "wrapped-title-" * 5
    home.session("s1", label=title, project_id="p")

    tui = case.pty()
    tui.read_for(3.0)
    tui.send("e", settle=0.8)
    grid = tui.grid()

    dialog_top = next(
        row for row in range(grid.rows) if "rename session" in grid.row(row)
    )
    first_row = dialog_top + 2
    case.check(
        "a long rename value wraps onto multiple rows",
        "prefix DELETE-ME suffix" in grid.row(first_row)
        and "wrapped-title" in grid.row(first_row + 1),
        "\n".join(grid.lines()),
    )

    delete_at = grid.row(first_row).index("DELETE-ME", grid.sidebar_width())
    # Character hit-testing uses insertion points: drag from the D to the
    # cell immediately after E, then Backspace removes the selected range.
    tui.drag((delete_at, first_row), (delete_at + len("DELETE-ME"), first_row))
    tui.send("\x7f", settle=0.6)
    edited = tui.grid()
    edited_top = next(
        row for row in range(edited.rows) if "rename session" in edited.row(row)
    )
    edited_field = "".join(edited.row(row) for row in range(edited_top + 2, edited_top + 4))
    case.check(
        "backspace removes the mouse-selected text",
        "DELETE-ME" not in edited_field and "prefix  suffix" in edited_field,
        edited_field,
    )

    tui.send("\r", settle=1.5)
    marker = home.read_marker("s1", "title.json")
    renamed = (marker or {}).get("title", "")
    case.check(
        "the edited wrapped title saves",
        "DELETE-ME" not in renamed and renamed.startswith("prefix  suffix"),
        repr(marker),
    )


run("rename_selection", body)
