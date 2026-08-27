"""A native-bridge Resume mints a new Session id asynchronously.

The compatibility receipt is only ``{"ok": true}``, so the TUI must follow
the exact replacement from stable launch identity. It must never fall back to
an unrelated first row during the source-to-replacement gap.
"""

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from harness import run  # noqa: E402


def summary(session_id, label, command, status, created_at):
    return {
        "id": session_id,
        "label": label,
        "command": command,
        "status": status,
        "pinned": False,
        "archived": False,
        "unread": False,
        "created_at": created_at,
    }


def sidebar(sessions):
    return {
        "projects": [
            {
                "id": "p",
                "name": "unpeel",
                "archived_count": 0,
                "sessions": sessions,
                "worktrees": [],
            }
        ]
    }


def body(case):
    home = case.home
    home.project("p", "unpeel", "/tmp")
    source_stamp = 1_754_300_000_000
    source = summary("source", "source stopped", "claude", "exited", source_stamp)
    decoy = summary(
        "decoy",
        "unrelated live",
        "unsupported-shell",
        "idle",
        source_stamp + 1,
    )
    home.session(
        "source",
        label="source stopped",
        command="claude",
        project_id="p",
        created_at=source_stamp,
        output="SOURCE_OLD_OUTPUT\r\n",
    )
    home.session(
        "decoy",
        label="unrelated live",
        command="unsupported-shell",
        project_id="p",
        created_at=source_stamp + 1,
        running=True,
        output="UNRELATED_FIRST_ROW\r\n",
    )
    app = case.app(sidebar=sidebar([decoy, source]))
    tui = case.pty()
    tui.read_for(3.0)

    grid = tui.grid()
    width = grid.sidebar_width()
    source_row = next(
        row for row in range(grid.rows) if "source stopped" in grid.row(row)[:width]
    )
    tui.click(5, source_row)
    tui.send("r", settle=0.8)
    requested = tui.wait_for(
        lambda: app.called("/mcp/restart-session", session_id="source"), timeout=10
    )
    case.check(
        "stopped Resume uses the native bridge",
        bool(requested),
        str(app.calls[-8:]),
    )

    # Native removes the source before publishing its asynchronous spawn.
    # Keep that gap visible for a complete bridge poll: the old bug selected
    # this decoy through first_listed_session as soon as source disappeared.
    app.sidebar = sidebar([decoy])
    gap = tui.wait_for(lambda: "source stopped" not in tui.sidebar(0.2), timeout=12)
    gap_preview = tui.preview_text(0.4)
    case.check(
        "the replacement gap never selects an unrelated first row",
        bool(gap) and "UNRELATED_FIRST_ROW" not in gap_preview,
        gap_preview[:320],
    )

    replacement = summary(
        "replacement",
        "source stopped",
        "claude --resume conversation",
        "starting",
        source_stamp,
    )
    home.session(
        "replacement",
        label="source stopped",
        command="claude --resume conversation",
        project_id="p",
        created_at=source_stamp,
        running=True,
        output="EXACT_BRIDGE_REPLACEMENT\r\n",
    )
    app.sidebar = sidebar([decoy, replacement])
    exact = tui.wait_for(
        lambda: "EXACT_BRIDGE_REPLACEMENT" in tui.preview_text(0.25), timeout=15
    )
    preview = tui.preview_text(0.4)
    case.check(
        "bridge Resume selects the exact replacement Session",
        bool(exact) and "UNRELATED_FIRST_ROW" not in preview,
        preview[:320],
    )


run("bridge_resume_selection", body)
