"""Unread is derived, not owned: a session that settled since its last read
receipt shows the blue dot, in every frontend. Looking at one writes the
receipt and tells the app; neither may flap.

Note the fixture shape — the *selected* session is legitimately marked read
the moment it's displayed, so the unread one must be a row the cursor is not
sitting on. Selection starts on the newest row."""

import sys, os, time

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from harness import run, UNREAD_DOT  # noqa: E402


def body(case):
    home = case.home
    home.project("p", "unpeel", "/tmp")
    # newest → selected at startup → read
    home.session("s-new", label="newest session", project_id="p",
                 created_at=1_754_400_000_000, settled=True)
    # older → not selected → its settle is unread
    home.session("s-old", label="older session", project_id="p",
                 created_at=1_754_300_000_000, settled=True)

    def bridge_sidebar(unread_ids):
        return {
            "projects": [
                {
                    "id": "p",
                    "name": "unpeel",
                    "archived_count": 0,
                    "worktrees": [],
                    "sessions": [
                        {"id": sid, "label": label, "command": "claude",
                         "status": "exited", "pinned": False, "archived": False,
                         "unread": sid in unread_ids, "created_at": created}
                        for sid, label, created in [
                            ("s-new", "newest session", 1_754_400_000_000),
                            ("s-old", "older session", 1_754_300_000_000),
                        ]
                    ],
                }
            ]
        }

    # The app claims BOTH are unread; the receipt on the selected one must win.
    app = case.app(sidebar=bridge_sidebar({"s-new", "s-old"}))

    tui = case.pty()
    tui.read_for(4.0)

    dotted = tui.expect(UNREAD_DOT)
    case.check("the unread dot renders", UNREAD_DOT in dotted, dotted[:200])
    case.check(
        "a receipt beats the app's claim",
        dotted.count(UNREAD_DOT) == 1,
        "both are claimed unread; only the unselected one may show a dot",
    )
    case.check(
        "the selected session gets a receipt",
        bool(home.read_marker("s-new", "read.json")),
    )

    tui.send("j", settle=1.5)  # select the older one
    receipt = home.read_marker("s-old", "read.json")
    case.check("looking at it writes a read receipt", bool(receipt), str(receipt))
    case.check(
        "the receipt carries a real timestamp",
        bool(receipt) and receipt.get("read_at", 0) > 1_700_000_000_000,
    )
    case.check("the app is told too", app.called("/mcp/mark-read", session_id="s-old"))

    cleared = tui.expect(absent=(UNREAD_DOT,), timeout=8)
    case.check("the dot clears once read", UNREAD_DOT not in cleared, cleared[:200])

    before = app.count("/mcp/mark-read")
    tui.read_for(3.0)
    case.check(
        "mark-read is not repeated",
        app.count("/mcp/mark-read") == before,
        "the app still claims unread; we must not re-notify on every poll",
    )

    # A NEW settle after the receipt genuinely re-marks it unread.
    time.sleep(1.1)
    home.settle("s-old")
    tui.send("k", settle=1.5)  # move the cursor away so it can go unread again
    remarked = tui.expect(UNREAD_DOT, timeout=10)
    case.check("new activity re-marks it unread", UNREAD_DOT in remarked, remarked[:200])


run("unread", body)
