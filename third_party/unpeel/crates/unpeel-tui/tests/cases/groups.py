"""Groups are plain organizational folders inside a project: child projects
with no worktree branch, rendered exactly like the inline worktree folder
rows but without the branch glyph. A session lands in a group via the
`project-override.json` marker in its session dir; a stale override (target
project gone) falls back to the manifest project rather than hiding the
session."""

import json
import re
import sys, os

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from harness import run  # noqa: E402


def sidebar_row(tui, text):
    grid = tui.grid()
    width = grid.sidebar_width()
    return next(row for row in range(grid.rows) if text in grid.row(row)[:width])


def body(case):
    home = case.home
    # A hand-ordered project list (what a drag writes): the group ranks
    # before the worktree, and the order stays put as sessions move.
    with open(home.path("project-order.json"), "w") as handle:
        json.dump(["main", "grp1", "wt1"], handle)
    home.write_state(
        {
            "projects": [
                {"id": "main", "name": "unpeel", "path": "/tmp/unpeel", "sort_order": 0},
                # A group: parent + is_folder, no worktree branch. sort_order
                # ranks it among the worktree folders (before feature-a).
                {
                    "id": "grp1",
                    "name": "Backlog",
                    "path": "/tmp/unpeel",
                    "parent_project_id": "main",
                    "is_folder": True,
                    "sort_order": 1,
                },
                {
                    "id": "wt1",
                    "name": "feature-a",
                    "path": "/tmp/wt-a",
                    "parent_project_id": "main",
                    "worktree_branch": "feature-a",
                    "sort_order": 2,
                },
            ]
        }
    )
    home.session("s-main", label="main session", project_id="main", settled=True)
    # Moved into the group by the shared marker — its manifest still says
    # "main", which is exactly the point of the override.
    home.session(
        "s-moved",
        label="moved session",
        project_id="main",
        settled=True,
        created_at=1_754_300_000_100,
    )
    home.marker(
        "s-moved",
        "project-override.json",
        {"project_id": "grp1", "moved_at": 1_754_300_000_200},
    )
    # A marker pointing at a project that no longer exists must be ignored.
    home.session(
        "s-stale",
        label="stale session",
        project_id="main",
        settled=True,
        created_at=1_754_300_000_050,
    )
    home.marker(
        "s-stale",
        "project-override.json",
        {"project_id": "ghost", "moved_at": 1_754_300_000_200},
    )
    home.session("s-a", label="branch a session", project_id="wt1", settled=True)

    tui = case.pty()
    tui.read_for(3.5)
    top = tui.sidebar()

    case.check(
        "a group renders as a folder row without the branch glyph",
        re.search(r"▸ Backlog\s+\(1\)", top) is not None
        and "⎇ Backlog" not in top,
        top[:250],
    )
    case.check(
        "the group is not a top-level project",
        "▾ Backlog" not in top,
        top[:250],
    )
    case.check(
        "groups sort among the worktree folders by sibling order",
        "Backlog" in top
        and "feature-a" in top
        and top.index("Backlog") < top.index("⎇ feature-a"),
        top[:250],
    )
    case.check(
        "groups default collapsed",
        "moved session" not in top,
        top[:250],
    )
    case.check(
        "a stale override falls back to the manifest project",
        "stale session" in top,
        top[:300],
    )
    case.check(
        "the parent's own sessions still show",
        "main session" in top,
        top[:300],
    )

    # "Move to" is a display-only filing action, so its destinations are
    # plain groups only. A Git worktree would imply changing checkout and
    # needs a separate restart/resume flow.
    grid = tui.grid()
    main_row = next(r for r in range(grid.rows) if "main session" in grid.row(r))
    tui.click(5, main_row, button=2)
    tui.expect("Move to")
    # Rename, Pin, Move to.
    tui.send("jj", settle=0.4)
    tui.send("\r", settle=0.6)
    move_menu = tui.expect("Move to Backlog")
    case.check(
        "move destinations include plain groups but exclude Git worktrees",
        "Move to Backlog" in move_menu and "Move to feature-a" not in move_menu,
        move_menu[:300],
    )
    tui.send("\x1b", settle=0.6)

    # A click unfolds the group in place, like worktrees. Locate the row from
    # the rendered sidebar so sibling/order changes cannot redirect it.
    tui.click(5, sidebar_row(tui, "Backlog"))
    tui.read_for(0.8)
    opened = tui.sidebar()
    case.check(
        "clicking the group expands it to the moved session",
        "▾ Backlog" in opened and "moved session" in opened,
        opened[:300],
    )
    case.check(
        "the moved session renders under its group row",
        "moved session" in opened
        and opened.index("Backlog") < opened.index("moved session"),
        opened[:300],
    )
    case.check(
        "the moved session left the parent's own list",
        opened.count("moved session") == 1
        and opened.index("moved session") < opened.index("main session"),
        opened[:300],
    )
    case.check(
        "the worktree folder next to it stays collapsed",
        "branch a session" not in opened,
        opened[:300],
    )

    # A second click folds it shut again — same toggle as a worktree folder.
    tui.click(5, sidebar_row(tui, "Backlog"))
    tui.read_for(0.8)
    closed = tui.sidebar()
    case.check(
        "clicking again collapses the group",
        "moved session" not in closed and re.search(r"▸ Backlog\s+\(1\)", closed),
        closed[:300],
    )

    # Clearing the marker moves the session back to its manifest project.
    os.unlink(home.path("app-sessions", "s-moved", "project-override.json"))
    tui.read_for(2.0)
    cleared = tui.sidebar()
    case.check(
        "removing the marker returns the session to the parent",
        "moved session" in cleared
        and re.search(r"▸ Backlog\s+\(0\)", cleared) is not None,
        cleared[:300],
    )
    case.check(
        "a collapsed empty group carries no new-session row",
        "+ New session" not in cleared,
        cleared[:300],
    )

    # An empty folder, expanded, offers the same way in as an empty
    # project — the "+ New session" row — and folds it away again shut.
    tui.click(5, sidebar_row(tui, "Backlog"))
    tui.read_for(0.8)
    empty_open = tui.sidebar()
    case.check(
        "an expanded empty group shows the new-session row",
        "▾ Backlog" in empty_open
        and "+ New session" in empty_open
        and empty_open.index("Backlog") < empty_open.index("+ New session")
        and empty_open.index("+ New session") < empty_open.index("feature-a"),
        empty_open[:300],
    )
    tui.click(5, sidebar_row(tui, "Backlog"))
    tui.read_for(0.8)
    empty_closed = tui.sidebar()
    case.check(
        "collapsing the empty group hides the row again",
        "+ New session" not in empty_closed,
        empty_closed[:300],
    )

    # Group rows get group-specific management verbs, not the generic project
    # removal inherited by worktree folders.
    grid = tui.grid()
    group_row = next(r for r in range(grid.rows) if "Backlog" in grid.row(r))
    tui.click(5, group_row, button=2)
    menu = tui.expect("Rename group")
    case.check(
        "group context menu names group-specific verbs",
        "Rename group" in menu
        and "Remove group" in menu
        and "Remove project" not in menu,
        menu[:350],
    )
    grid = tui.grid()
    rename_row = next(r for r in range(grid.rows) if "Rename group" in grid.row(r))
    new_session_row = next(r for r in range(grid.rows) if "New session" in grid.row(r))
    case.check(
        "rename group follows new session",
        rename_row == new_session_row + 1,
        menu[:350],
    )

    rename_col = grid.row(rename_row).index("Rename group") + 2
    tui.click(rename_col, rename_row)
    tui.expect("rename group")
    tui.backspace(len("Backlog"))
    tui.type("Research")
    tui.send("\r", settle=2.0)
    renamed = tui.sidebar()
    case.check(
        "a group can be renamed from its context menu",
        "Research" in renamed and "Backlog" not in renamed,
        renamed[:300],
    )
    with open(home.path("app-state.json")) as handle:
        state = json.load(handle)
    case.check(
        "group rename persists in shared state",
        next(p for p in state["projects"] if p["id"] == "grp1")["name"] == "Research",
    )

    # Put the session back into the group, then remove the group. Removal
    # must preserve the conversation by rehoming it to the parent archive.
    home.marker(
        "s-moved",
        "project-override.json",
        {"project_id": "grp1", "moved_at": 1_754_300_000_300},
    )
    tui.read_for(2.0)
    grid = tui.grid()
    group_row = next(r for r in range(grid.rows) if "Research" in grid.row(r))
    tui.click(5, group_row, button=2)
    grid = tui.grid()
    remove_row = next(r for r in range(grid.rows) if "Remove group" in grid.row(r))
    remove_col = grid.row(remove_row).index("Remove group") + 2
    tui.click(remove_col, remove_row)
    confirm = tui.expect("archive 1 session")
    case.check(
        "removing a group confirms its sessions will be archived",
        "archive 1 session" in confirm and "Remove group" in confirm,
        confirm[:350],
    )
    grid = tui.grid()
    confirm_row = next(
        r
        for r in range(grid.rows)
        if "Remove group" in grid.row(r) and "archive" not in grid.row(r)
    )
    confirm_col = grid.row(confirm_row).index("Remove group") + 2
    tui.click(confirm_col, confirm_row)
    tui.read_for(4.0)

    with open(home.path("app-state.json")) as handle:
        state = json.load(handle)
    with open(home.path("app-sessions", "s-moved", "project-override.json")) as handle:
        moved_override = json.load(handle)
    case.check(
        "removing a group deletes its project record",
        all(p["id"] != "grp1" for p in state["projects"]),
    )
    case.check(
        "group sessions are archived and remain reachable under the parent",
        os.path.exists(home.path("app-sessions", "s-moved", "archived.json"))
        and moved_override["project_id"] == "main"
        and os.path.isdir(home.path("app-sessions", "s-moved")),
    )


run("groups", body)
