"""Version skew between the TUI and the desktop app.

Users update the two independently — a newer `unpeel` will meet an older app
that has never heard of /mcp/sidebar, /mcp/archive-session or /mcp/mark-read.
Every one of those must degrade to the app-less path rather than leaving the
user with a dead UI or a verb that silently does nothing."""

import sys, os

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from harness import run  # noqa: E402


def body(case):
    home = case.home
    home.project("p", "unpeel", "/tmp")
    # Running, because Archive is only offered for a live session — a
    # stopped one is already filed and shows Remove instead.
    home.session("s-one", label="first session", project_id="p",
                 created_at=1_754_400_000_000, settled=True, running=True)
    case.host("s-one")
    home.session("s-two", label="second session", project_id="p",
                 created_at=1_754_300_000_000, settled=True)

    # An older app: it answers the routes it shipped with, and 404s the rest.
    old_app = case.app(
        fail_routes=(
            "/mcp/sidebar",
            "/mcp/archive-session",
            "/mcp/restore-session",
            "/mcp/mark-read",
        )
    )

    tui = case.pty()
    tui.read_for(4.0)
    listed = tui.expect("first session", "second session")

    case.check(
        "the sidebar falls back to disk when /mcp/sidebar 404s",
        "first session" in listed and "second session" in listed,
        listed[:200],
    )
    case.check("the app was actually asked", old_app.count("/mcp/sidebar") > 0)

    # Unread still works: it is derived from shared files, not the app.
    case.check(
        "unread survives a missing mark-read route",
        bool(home.read_marker("s-one", "read.json")),
        "the receipt is the source of truth, the bridge call is a courtesy",
    )

    # Archiving must fall back to writing the shared marker itself.
    tui.send("s", settle=2.5)
    archived = tui.wait_for(lambda: home.has_marker("s-one", "archived.json"), timeout=15)
    case.check(
        "archive falls back to the shared marker",
        archived,
        "an old app 404s archive-session; the TUI must still file the session",
    )

    # And the UI must say something rather than looking frozen.
    case.check(
        "the sidebar keeps working after the fallback",
        "second session" in tui.expect("second session"),
    )

    # Restart is a route the old app DOES have — it must still be used.
    tui.send("j", settle=0.8)
    tui.send("r", settle=2.0)
    case.check(
        "routes the old app has are still used",
        old_app.called("/mcp/restart-session"),
    )


run("compat_bridge", body)
