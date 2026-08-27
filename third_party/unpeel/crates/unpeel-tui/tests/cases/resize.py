"""Terminal geometry: the session's PTY is resized to the preview pane, and
a phone that takes the grid resizes it for real — then hands it back."""

import sys, os

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from harness import run, mobile_request  # noqa: E402


def body(case):
    home = case.home
    home.project("p", "unpeel", "/tmp")
    home.session("live-1", label="live session", project_id="p", running=True)
    host = case.host("live-1", cols=80, rows=24)
    token = home.pair_device()
    port = home.reserve_mobile_port()

    tui = case.pty(rows=45, cols=150)
    tui.read_for(5.0)

    fitted = tui.wait_for(lambda: host.resizes, timeout=15)
    case.check("the session is fitted to the pane", bool(fitted), str(host.resizes))
    if host.resizes:
        cols, rows = host.resizes[-1]
        case.check(
            "the fit matches the preview pane, not the window",
            0 < cols < 150 and 0 < rows < 45,
            f"{cols}x{rows} inside a 150x45 window",
        )

    # Widening the window re-fits the session.
    before = list(host.resizes)
    tui.resize_window(170, 50)
    tui.read_for(2.0)
    case.check(
        "resizing the window re-fits the session",
        len(host.resizes) > len(before),
        str(host.resizes[-3:]),
    )

    # ── the phone takes the grid ──
    status, _ = mobile_request(port, "/mobile/resize-desktop", token, method="POST",
                               # the phone protocol spells it `columns`
                               body={"sessionID": "live-1", "columns": 40, "rows": 20})
    case.check("the phone's fit is accepted", status == 200, str(status))
    took = tui.wait_for(lambda: host.resizes and host.resizes[-1] == (40, 20), timeout=10)
    case.check(
        "the PTY really is resized to the phone's grid",
        took,
        f"last resize {host.resizes[-1] if host.resizes else None}",
    )
    case.check(
        "the UI says the phone owns the grid",
        "mobile" in tui.expect("mobile").lower(),
    )

    # ── the fight: while the phone owns the grid, the TUI must NOT keep
    #    resizing the PTY back to its pane. Watch several ticks. ──
    baseline = len(host.resizes)
    tui.read_for(2.0)                       # idle
    for _ in range(4):                      # AND passive sidebar navigation
        tui.send("j", settle=0.2)
        tui.send("k", settle=0.2)
    tui.click(5, 1)                         # structural clicks stay passive
    tui.click(5, 1)
    tui.read_for(1.0)
    fights = [r for r in host.resizes[baseline:] if r != (40, 20)]
    case.check(
        "the TUI does not fight the phone's width, even during interaction",
        not fights,
        f"TUI resized the shared PTY {len(fights)}x while the phone owned it: {fights[:4]}",
    )

    # ── and hands it back ──
    status, _ = mobile_request(port, "/mobile/resize-desktop", token, method="POST",
                               body={"sessionID": "live-1", "clear": True})
    case.check("clearing is accepted", status == 200, str(status))
    returned = tui.wait_for(
        lambda: host.resizes and host.resizes[-1] != (40, 20), timeout=10
    )
    case.check("the grid returns to the pane's size", returned, str(host.resizes[-2:]))


run("resize", body)
