"""Session verbs while the desktop app is running: every one routes over
the /mcp bridge with the shared auth token, and the destructive one asks
first."""

import sys, os

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from harness import run  # noqa: E402


def body(case):
    home = case.home
    home.project("p", "unpeel", "/tmp")
    home.session(
        "s-one",
        label="first session",
        command="claude",
        project_id="p",
        running=True,
        extra_manifest={"host_protocol_version": 3},
    )
    home.session(
        "s-old-host",
        label="old host session",
        command="claude",
        project_id="p",
        running=True,
        extra_manifest={"host_protocol_version": 2},
    )
    home.session(
        "s-active",
        label="active agent session",
        command="claude",
        project_id="p",
        running=True,
        extra_manifest={
            "host_protocol_version": 3,
            "runtime": {"currentObservation": {"id": "claude"}},
        },
    )
    home.session(
        "s-pending",
        label="pending launch session",
        command="claude",
        project_id="p",
        running=True,
        extra_manifest={
            "host_protocol_version": 3,
            "runtime_launch_pending": True,
        },
    )
    home.session("s-two", label="second session", project_id="p")
    live_host = case.host("s-one")

    app = case.app()
    tui = case.pty()
    tui.read_for(3.0)

    grid = tui.grid()
    live_row = next(row for row in range(grid.rows) if "first session" in grid.row(row))
    tui.click(5, live_row, button=2)
    menu = tui.expect("Resume Agent")
    case.check(
        "a returned managed Session labels the context action Resume Agent",
        "Resume Agent" in menu and "Restart Agent" not in menu,
        menu[:240],
    )
    tui.send("\x1b", settle=0.5)

    grid = tui.grid()
    old_row = next(row for row in range(grid.rows) if "old host session" in grid.row(row))
    tui.click(5, old_row, button=2)
    old_menu = tui.expect("Copy transcript")
    case.check(
        "an old live Host offers neither Resume Agent nor Resume",
        "Resume Agent" not in old_menu and "Restart Agent" not in old_menu,
        old_menu[:240],
    )
    tui.send("\x1b", settle=0.3)
    tui.send("r", settle=0.8)
    case.check(
        "r fails closed for an old live Host",
        "Resume Agent is unavailable for this live Host" in tui.screen(),
    )

    grid = tui.grid()
    active_row = next(row for row in range(grid.rows) if "active agent session" in grid.row(row))
    tui.click(5, active_row, button=2)
    active_menu = tui.expect("Copy transcript")
    case.check(
        "an active managed runtime has no lifecycle action",
        "Resume Agent" not in active_menu and "Restart Agent" not in active_menu,
        active_menu[:240],
    )
    tui.send("\x1b", settle=0.3)
    tui.send("r", settle=0.5)
    case.check(
        "r explains why an active runtime has no action",
        "managed agent is still active" in tui.screen(),
    )

    grid = tui.grid()
    pending_row = next(
        row for row in range(grid.rows) if "pending launch session" in grid.row(row)
    )
    tui.click(5, pending_row, button=2)
    pending_menu = tui.expect("Copy transcript")
    case.check(
        "an in-place launch still pending has no duplicate Resume Agent action",
        "Resume Agent" not in pending_menu,
        pending_menu[:240],
    )
    tui.send("\x1b", settle=0.3)

    grid = tui.grid()
    live_row = next(row for row in range(grid.rows) if "first session" in grid.row(row))
    tui.click(5, live_row, button=2)
    tui.expect("Resume Agent")
    tui.send("\x1b", settle=0.3)

    tui.send("r", settle=1.5)
    case.check(
        "Resume Agent stays in the hosted terminal",
        not app.called("/mcp/restart-session", session_id="s-one")
        and home.manifests().get("s-one", {}).get("state") == "running",
        str(home.manifests().get("s-one")),
    )
    case.check(
        "Resume Agent sends one generation-guarded Host command",
        live_host.resume_agent_generations == [0],
        str(live_host.resume_agent_generations),
    )
    case.check("Resume Agent reports back", "resuming agent" in tui.screen())

    tui.send("s", settle=1.5)
    case.check(
        "stop archives through the app",
        app.called("/mcp/archive-session", session_id="s-one"),
    )

    grid = tui.grid()
    stopped_row = next(row for row in range(grid.rows) if "second session" in grid.row(row))
    tui.click(5, stopped_row, button=2)
    tui.expect("Resume")
    tui.send("\x1b", settle=0.3)
    tui.send("x", settle=0.8)
    confirm = tui.screen()
    case.check("remove asks first", "Remove" in confirm and "y/n" in confirm, confirm[:120])

    tui.send("n", settle=0.8)
    case.check(
        "declining removes nothing",
        not app.called("/mcp/close-session"),
    )

    tui.send("x", settle=0.8)
    tui.send("y", settle=1.5)
    case.check(
        "confirming removes through the app",
        app.called("/mcp/close-session", session_id="s-two"),
    )

    tui.send("p", settle=1.0)
    case.check(
        "pin is a shared contract, not a bridge call",
        home.state().get("pinned_sessions") is not None,
    )


run("verbs", body)
