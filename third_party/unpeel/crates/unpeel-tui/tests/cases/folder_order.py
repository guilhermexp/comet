"""Groups are structural headers; group/worktree siblings drag-sort together."""

import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from harness import run  # noqa: E402


def body(case):
    home = case.home
    home.write_state(
        {
            "projects": [
                {"id": "root", "name": "unpeel", "path": "/tmp", "sort_order": 0},
                {
                    "id": "group",
                    "name": "Ideas",
                    "path": "/tmp",
                    "parent_project_id": "root",
                    "is_folder": True,
                    "sort_order": 0,
                },
                {
                    "id": "wt-a",
                    "name": "feature-a",
                    "path": "/tmp/wt-a",
                    "parent_project_id": "root",
                    "worktree_branch": "feature-a",
                    "sort_order": 1,
                },
                {
                    "id": "wt-b",
                    "name": "feature-b",
                    "path": "/tmp/wt-b",
                    "parent_project_id": "root",
                    "worktree_branch": "feature-b",
                    "sort_order": 2,
                },
            ]
        }
    )
    home.session("s-group", label="group session", project_id="group", settled=True)
    home.session("s-a", label="worktree a session", project_id="wt-a", settled=True)
    home.session("s-b", label="worktree b session", project_id="wt-b", settled=True)
    home.session("s-root", label="root session", project_id="root", settled=True)

    tui = case.pty()
    tui.read_for(3.5)
    initial = tui.sidebar()
    case.check(
        "groups and worktrees share sibling order",
        initial.index("Ideas") < initial.index("feature-a") < initial.index("feature-b"),
        initial[:300],
    )

    # From the parent's session, three Up presses would land on the group if
    # it participated in selection. It must instead stop on the first real
    # worktree; Enter therefore opens feature-a, not Ideas.
    grid = tui.grid()
    sidebar_width = grid.sidebar_width()
    root_row = next(
        row
        for row in range(grid.rows)
        if "root session" in grid.row(row)[:sidebar_width]
    )
    tui.click(6, root_row)
    tui.send("kkk", settle=0.7)
    tui.send("\r", settle=0.9)
    keyboard = tui.sidebar()
    case.check(
        "keyboard traversal skips plain groups",
        "worktree a session" in keyboard and "group session" not in keyboard,
        keyboard[:320],
    )
    tui.send("\r", settle=0.6)  # collapse feature-a again

    # Drag the group down over feature-b. A group press is a possible drag,
    # not a selection or immediate disclosure toggle.
    grid = tui.grid()
    sidebar_width = grid.sidebar_width()
    group_row = next(
        row for row in range(grid.rows) if "Ideas" in grid.row(row)[:sidebar_width]
    )
    target_row = next(
        row for row in range(grid.rows) if "feature-b" in grid.row(row)[:sidebar_width]
    )
    tui.drag((6, group_row), (6, target_row))
    reordered = tui.sidebar()
    case.check(
        "dragging reorders group and worktree siblings",
        reordered.index("feature-a") < reordered.index("feature-b") < reordered.index("Ideas"),
        reordered[:320],
    )
    case.check(
        "dragging a group neither selects nor expands it",
        "group session" not in reordered,
        reordered[:320],
    )

    order_path = home.path("project-order.json")
    with open(order_path) as handle:
        order = json.load(handle)
    case.check(
        "folder order persists in the shared project rank file",
        set(order) == {"root", "group", "wt-a", "wt-b"}
        and order.index("wt-b") < order.index("group"),
        repr(order),
    )

    # A normal click still opens the group, but its row never gets the
    # selection marker used by worktrees.
    grid = tui.grid()
    sidebar_width = grid.sidebar_width()
    group_row = next(
        row for row in range(grid.rows) if "Ideas" in grid.row(row)[:sidebar_width]
    )
    tui.click(6, group_row)
    opened = tui.grid()
    group_line = next(line for line in opened.lines() if "Ideas" in line)
    case.check(
        "clicking a group toggles it without selecting it",
        "group session" in opened.sidebar() and "▌" not in group_line[: opened.sidebar_width()],
        "\n".join(opened.lines()[:12]),
    )


run("folder_order", body)
