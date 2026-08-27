"""Session order is one live contract between the desktop and TUI.

The desktop feed can be one poll behind the shared file, so both a TUI drag
and an app-side write must win over that stale payload immediately."""

import json
import os
import sys
import urllib.request

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from harness import run  # noqa: E402


def session(session_id, label, created_at):
    return {
        "id": session_id,
        "label": label,
        "command": "claude",
        "status": "idle",
        "pinned": False,
        "archived": False,
        "unread": False,
        "created_at": created_at,
    }


def body(case):
    home = case.home
    home.project("p", "unpeel", "/tmp")
    sidebar = {
        "projects": [{
            "id": "p",
            "name": "unpeel",
            "archived_count": 0,
            "sessions": [
                session("s-c", "gamma", 3),
                session("s-b", "bravo", 2),
                session("s-a", "alpha", 1),
            ],
            "worktrees": [],
        }]
    }
    app = case.app(sidebar=sidebar)
    tui = case.pty()
    tui.read_for(3.0)

    before = tui.sidebar()
    case.check(
        "the TUI is rendering the app's sidebar feed",
        before.index("gamma") < before.index("bravo") < before.index("alpha")
        and app.count("/mcp/sidebar") > 0,
        before[:200],
    )

    # Move gamma down onto alpha. The mock desktop deliberately keeps serving
    # its old order, so the shared file must hold the TUI's settled result.
    tui.drag((4, 2), (4, 4))
    tui.read_for(1.0)
    after_drag = tui.sidebar()
    case.check(
        "a TUI reorder stays put over a stale app feed",
        after_drag.index("bravo") < after_drag.index("alpha") < after_drag.index("gamma"),
        after_drag[:200],
    )
    case.check(
        "the TUI announces its order write to the app",
        any(
            path == "/state-changed" and body.get("change") == "order"
            for path, _token, body in app.calls
        ),
        str(app.calls[-8:]),
    )

    # Now stand in for an app-side drag: write a different rank list and ping
    # the TUI's existing state-bus route. It must beat the same stale feed.
    order_path = home.path("session-order.json")
    with open(order_path, "w") as handle:
        json.dump({"p": ["s-a", "s-c", "s-b"]}, handle)
    with open(home.path("app-ports")) as handle:
        tui_ports = [int(line) for line in handle if int(line) != app.port]
    for port in tui_ports:
        request = urllib.request.Request(
            f"http://127.0.0.1:{port}/state-changed",
            data=b'{"change":"order"}',
            headers={"Content-Type": "application/json"},
            method="POST",
        )
        with urllib.request.urlopen(request, timeout=2):
            pass
    tui.read_for(0.5)
    after_app = tui.sidebar()
    case.check(
        "an app reorder is adopted immediately by the TUI",
        after_app.index("alpha") < after_app.index("gamma") < after_app.index("bravo"),
        after_app[:200],
    )


run("session_order_sync", body)
