"""Worktrees are children of their project, never loose top-level rows:
one collapsible folder row per child sits directly under the parent's
header, above the parent's own sessions. Folders default collapsed; ⏎ or a
click folds/unfolds in place, and a collapsed folder still surfaces
attention from the sessions inside it."""

import re
import sys, os

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from harness import run  # noqa: E402


def body(case):
    home = case.home
    home.write_state(
        {
            "projects": [
                {"id": "main", "name": "unpeel", "path": "/tmp/unpeel"},
                {
                    "id": "wt1",
                    "name": "feature-a",
                    "path": "/tmp/wt-a",
                    "parent_project_id": "main",
                    "worktree_branch": "feature-a",
                },
                {
                    "id": "wt2",
                    "name": "feature-b",
                    "path": "/tmp/wt-b",
                    "parent_project_id": "main",
                    "worktree_branch": "feature-b",
                },
            ]
        }
    )
    home.session("s-main", label="main session", project_id="main", settled=True)
    home.session("s-a", label="branch a session", project_id="wt1", settled=True)
    # feature-b's session is live with an agent-drawn menu up, so its
    # collapsed folder must carry the attention diamond.
    home.session(
        "s-b",
        label="branch b session",
        project_id="wt2",
        running=True,
        extra_manifest={"menu_prompt_active": True},
    )

    tui = case.pty()
    tui.read_for(3.5)
    top = tui.sidebar()

    case.check(
        "worktrees are not top-level projects",
        "▾ feature-a" not in top and "▾ feature-b" not in top,
        top[:200],
    )
    case.check(
        "one folder row per child, with its parenthesized session count",
        # The plain muted count sits at the right edge, date-style, so padding
        # separates it from the name.
        re.search(r"▸ ⎇ feature-a\s+\(1\)", top) is not None
        and re.search(r"▸ ⎇ feature-b.*?\(1\)", top) is not None,
        top[:250],
    )
    case.check(
        "folder rows sit above the parent's sessions",
        top.index("⎇ feature-a") < top.index("main session"),
    )
    case.check(
        "folders default collapsed",
        "branch a session" not in top and "branch b session" not in top,
        top[:250],
    )
    case.check("the parent's own session still shows", "main session" in top)
    case.check(
        "a project with sessions has no new-session row",
        "+ New session" not in top,
        top[:200],
    )
    case.check(
        "a collapsed folder surfaces attention from inside",
        "feature-b ◆" in top,
        top[:250],
    )

    # Keyboard: two up from the parent's session lands on feature-a's
    # folder row (folders traverse like any selectable row), and ⏎ unfolds
    # it in place.
    tui.send("k", settle=0.5)
    tui.send("k", settle=0.5)
    tui.send("\r", settle=1.0)
    opened = tui.sidebar()
    case.check(
        "⏎ expands the folder to its sessions",
        "▾ ⎇ feature-a" in opened and "branch a session" in opened,
        opened[:300],
    )
    case.check(
        "the session renders under its folder row",
        opened.index("⎇ feature-a") < opened.index("branch a session"),
    )
    case.check(
        "the parent's sessions stay visible throughout",
        "main session" in opened
        and opened.index("branch a session") < opened.index("main session"),
        opened[:300],
    )
    case.check(
        "the other folder stays collapsed",
        "branch b session" not in opened,
        opened[:300],
    )

    # Worktree sessions indent one level deeper than the parent's own
    # (settled sessions carry no status glyph, so compare label columns —
    # sidebar columns only, or the preview pane's words shadow the rows).
    grid = tui.grid()
    width = grid.sidebar_width()
    lines = [line[:width] for line in grid.lines()]
    wt_row = next(line for line in lines if "branch a session" in line)
    main_row = next(line for line in lines if "main session" in line)
    case.check(
        "worktree sessions indent deeper than project sessions",
        wt_row.index("branch a session") > main_row.index("main session"),
        f"wt={wt_row!r} main={main_row!r}",
    )

    # ⏎ again folds it shut.
    tui.send("\r", settle=1.0)
    closed = tui.sidebar()
    case.check(
        "⏎ collapses it again",
        "branch a session" not in closed and "▸ ⎇ feature-a" in closed,
        closed[:300],
    )

    # A click toggles too: the folder rows sit right under the header
    # (screen row 1 = header, row 2 = feature-a).
    tui.click(5, 2)
    tui.read_for(0.8)
    case.check(
        "clicking the folder row expands it",
        "branch a session" in tui.sidebar(),
        tui.sidebar()[:300],
    )
    tui.click(5, 2)
    tui.read_for(0.8)
    case.check(
        "clicking again collapses it",
        "branch a session" not in tui.sidebar(),
        tui.sidebar()[:300],
    )
    case.check(
        "the parent's session survived all of it",
        "main session" in tui.sidebar(),
    )


run("worktrees", body)
