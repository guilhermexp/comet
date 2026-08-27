"""The sidebar's top-right activity control mirrors the native app: a
global spinner opens active + unread rows, and its footer opens All recent."""

import json
import os
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from harness import (  # noqa: E402
    SPINNER_CHARS,
    UNREAD_DOT,
    post_hook,
    run,
    tui_hook_port,
)


def click_text(tui, text, predicate=lambda _line: True):
    grid = tui.grid()
    candidates = [
        (row, line)
        for row, line in enumerate(grid.lines())
        if text in line and predicate(line)
    ]
    if not candidates:
        return False
    # Session actions are below the sidebar copy in this anchored popup. Use
    # the last occurrence so the duplicate sidebar title cannot turn the
    # click into an outside-popover dismissal.
    row, line = candidates[-1]
    tui.click(line.find(text) + 1, row)
    return True


def body(case):
    home = case.home
    home.project("p", "unpeel", "/tmp")
    # These parked fixture processes use deliberately old creation stamps.
    # Keep the independent day-old idle cleanup policy out of this UI case.
    state = home.state()
    state["auto_stop_archive_minutes"] = 0
    with open(home.path("app-state.json"), "w") as handle:
        json.dump(state, handle, indent=2)
    home.session("s1", label="alpha history", project_id="p")
    home.session("s2", label="beta working", project_id="p", running=True)
    home.session("s3", label="gamma finished", project_id="p", running=True)

    # Seed one native-format history row: All recent must consume the shared
    # feed, while the compact dropdown must exclude this ordinary read-idle
    # Session.
    with open(home.path("activity-log.jsonl"), "w") as handle:
        handle.write(
            json.dumps(
                {
                    "id": "event-alpha",
                    "session_id": "s1",
                    "kind": "finished",
                    "at": int(time.time() * 1000),
                    "title": "alpha history",
                    "command": "claude --dangerously-skip-permissions",
                    "project_id": "p",
                    "project_name": "unpeel",
                }
            )
            + "\n"
        )

    tui = case.pty()
    tui.read_for(3.0)
    port = tui_hook_port(home)
    post_hook(port, "s2", "Start")
    tui.wait_for(lambda: any(c in tui.frame(0.4) for c in SPINNER_CHARS), timeout=10)

    grid = tui.grid()
    sidebar_width = grid.sidebar_width()
    top = grid.row(0)[:sidebar_width]
    case.check(
        "any working session spins at the sidebar top-right",
        any(c in top for c in SPINNER_CHARS),
        repr(top),
    )

    # Default sidebar width is 36: click the generous five-cell top-right
    # target immediately before its corner.
    tui.click(sidebar_width - 3, 0)
    popup = tui.expect("All recent")
    case.check(
        "the activity control opens active rows and the history footer",
        "recent activity" in popup and "beta working" in popup and "All recent" in popup,
        popup[:300],
    )
    case.check(
        "read-idle history stays out of the compact dropdown",
        popup.count("alpha history") == 1,  # its one sidebar copy only
        f"alpha x{popup.count('alpha history')}",
    )

    case.check("the All recent footer is clickable", click_text(tui, "All recent"))
    recent = tui.expect("Finished just now")
    case.check(
        "All recent is a main-pane history page backed by the shared log",
        "All recent" in recent and "alpha history" in recent and "Finished just now" in recent,
        recent[:320],
    )
    tui.send("\x1b", settle=0.5)

    # A job that settles while another Session is selected moves beneath the
    # active group as an unread blue-dot row. Clicking it reveals the Session
    # and publishes the shared read receipt.
    post_hook(port, "s3", "Start")
    tui.wait_for(
        lambda: any(
            "gamma finished" in line and any(c in line for c in SPINNER_CHARS)
            for line in tui.grid(0.4).lines()
        ),
        timeout=10,
    )
    home.settle("s3")
    post_hook(port, "s3", "Stop")
    tui.wait_for(lambda: UNREAD_DOT in tui.frame(0.4), timeout=12)
    sidebar_width = tui.grid().sidebar_width()
    tui.click(sidebar_width - 3, 0)
    popup = tui.expect("All recent")
    tui.wait_for(
        lambda: sum(
            "gamma finished" in line for line in tui.grid(0.4).lines()
        )
        >= 2,
        timeout=10,
    )
    popup = tui.grid(0.4).text()
    beta = popup.rfind("beta working")
    gamma = popup.rfind("gamma finished")
    case.check(
        "active rows precede unread-finished rows",
        -1 != beta < gamma,
        f"beta={beta} gamma={gamma}",
    )
    gamma_lines = [
        (row, line)
        for row, line in enumerate(tui.grid().lines())
        if "gamma finished" in line
    ]
    case.check(
        "clicking an unread row reveals it",
        click_text(tui, "gamma finished"),
        repr(gamma_lines),
    )
    tui.wait_for(lambda: home.has_marker("s3", "read.json"), timeout=8)
    case.check(
        "revealing from the dropdown marks the Session read",
        home.has_marker("s3", "read.json"),
        f"matches={gamma_lines!r} files={os.listdir(home.path('app-sessions', 's3'))!r}",
    )

    sidebar_width = tui.grid().sidebar_width()
    tui.click(sidebar_width - 3, 0)
    read_popup = tui.expect("All recent")
    case.check(
        "a read settled Session leaves the compact dropdown",
        read_popup.count("gamma finished") == 1,  # sidebar only
        f"gamma x{read_popup.count('gamma finished')}",
    )


run("activity-menu", body)
