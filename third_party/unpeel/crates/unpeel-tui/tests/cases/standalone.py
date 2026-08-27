"""No desktop app anywhere: the TUI must host sessions itself. This is the
case that proves `unpeel` is a complete Unpeel rather than a viewer — and
the one a Linux box depends on entirely."""

import sys, os

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from harness import run  # noqa: E402


def body(case):
    home = case.home
    home.project("p", "unpeel", "/tmp")
    home.preset(label="cat", command="cat")
    home.session("fixture-3", label="blank session", command="", project_id="p",
                 created_at=1_754_300_000_000)

    case.check("no app is running", home.ports() == [])

    tui = case.pty()
    tui.read_for(3.0)

    # Resume an exited blank terminal with no app to bridge to.
    tui.send("r", settle=1.0)
    live = tui.wait_for(
        lambda: (
            home.running_sessions()
            if len(home.running_sessions()) == 1 and "fixture-3" not in home.manifests()
            else None
        ),
        timeout=25,
    )
    case.check("Resume spawns a real host app-lessly", bool(live), str(home.manifests().keys()))

    if live:
        new_id = next(iter(live))
        manifest = live[new_id]["session"]
        case.check(
            "Resume carries the session's identity",
            manifest["label"] == "blank session"
            and manifest["created_at"] == 1_754_300_000_000
            and manifest["command"] == "",
            str(manifest),
        )
        case.check(
            "the host is real (control socket bound)",
            os.path.exists(home.path("app-sessions", new_id, "session.sock")),
        )

    # New session from the preset picker.
    tui.send("n", settle=1.2)
    tui.send("\r", settle=1.0)
    two = tui.wait_for(
        lambda: len(home.running_sessions()) == 2 and home.running_sessions(), timeout=25
    )
    case.check("a new session spawns from a preset", bool(two))

    # Stop, then remove, both without an app.
    tui.send("s", settle=1.0)
    tui.send("y", settle=3.0)
    tui.send("x", settle=1.0)
    tui.send("y", settle=1.0)
    one_left = tui.wait_for(lambda: len(home.running_sessions()) == 1, timeout=25)
    case.check("stop and remove work app-lessly", bool(one_left), str(home.running_sessions()))

    # The TUI registered a hook port of its own, so agents can report in.
    case.check("it serves the hook bus itself", len(home.ports()) == 1, str(home.ports()))


run("standalone", body)
