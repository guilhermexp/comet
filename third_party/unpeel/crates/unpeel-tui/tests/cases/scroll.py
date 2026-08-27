"""The sidebar wheel scrolls the viewport — it must not move the selection.

Scrolling to look around used to switch which session the preview showed
(and re-fit its PTY), which makes browsing a long list impossible.

Fixture note: one session per project rather than many in one, because the
stopped-session window folds older stopped rows into Archive (desktop
parity) and would stop the list overflowing."""

import sys, os, re

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from harness import run  # noqa: E402


def body(case):
    home = case.home
    for index in range(8):
        home.project(f"p{index}", f"project-{index:02d}", "/tmp")
        home.session(
            f"s-{index:02d}",
            label=f"session {index:02d}",
            project_id=f"p{index}",
            created_at=1_754_400_000_000 - index * 1000,
            output=f"content of session {index:02d}\r\n",
        )

    # A short window against 16 rows of sidebar, so it genuinely overflows.
    tui = case.pty(rows=12, cols=120)
    tui.read_for(3.0)

    first = tui.sidebar()
    case.check("the first project is at the top", "project-00" in first, first[:160])
    case.check("the list overflows the window", "project-07" not in first, first[:160])
    case.check(
        "the first session is selected and previewed",
        "session 00 is stopped" in tui.preview_text(),
        tui.preview_text()[:120],
    )

    tui.scroll(5, 5, up=False, times=4)
    scrolled = tui.sidebar()
    case.check("the wheel scrolls the sidebar", "project-00" not in scrolled, scrolled[:160])
    case.check("it reveals rows further down", "project-07" in scrolled, scrolled[:160])
    case.check(
        "the selection does not follow the wheel",
        "session 00 is stopped" in tui.preview_text(),
        "scrolling must not switch the previewed session",
    )
    case.check(
        "and the viewport stays where it was put",
        "project-00" not in tui.sidebar(),
        "a later frame must not snap back to the selection",
    )

    tui.scroll(5, 5, up=True, times=8)
    case.check("scrolling back up returns to the top", "project-00" in tui.sidebar())

    # The keyboard still moves the selection AND brings it into view.
    # Which session N presses lands on depends on how many rows each project
    # contributes, so assert the property rather than a specific row.
    for _ in range(7):
        tui.send("j", settle=0.2)
    tui.read_for(0.8)
    preview = tui.preview_text()
    match = re.search(r"session (\d\d) is stopped", preview)
    case.check("arrowing moves the selection", match is not None, preview[:160])
    if match:
        case.check(
            "and scrolls it into view",
            f"session {match.group(1)}" in tui.sidebar(),
            tui.sidebar()[:200],
        )


run("scroll", body)
