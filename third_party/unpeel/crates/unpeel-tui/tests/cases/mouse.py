"""Mouse: child mouse modes receive reports; plain active terminals retain
focus while a drag selects and copies from the rendered grid."""

import sys, os

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from harness import run  # noqa: E402


def body(case):
    home = case.home
    home.project("p", "unpeel", "/tmp")
    home.session("live-1", label="live session", project_id="p", running=True)
    host = case.host("live-1", mouse_reporting=True)

    tui = case.pty()
    tui.read_for(4.0)

    case.check(
        "mouse capture is on at start",
        any(mode in tui.buffer for mode in (b"\x1b[?1000h", b"\x1b[?1002h", b"\x1b[?1003h")),
    )

    tui.send("\r", settle=1.5)  # focus the session
    tui.click(79, 9)
    forwarded = host.written()
    case.check("click is forwarded to the session", "\x1b[<0;" in forwarded, forwarded[:80])
    case.check(
        "coordinates are pane-relative",
        "\x1b[<0;43;9M" in forwarded,
        "a click at screen (80,10) lands inside the pane, not at screen coords",
    )
    case.check("press and release both arrive", forwarded.rstrip().endswith("m"))

    before = len(host.writes)
    tui.send(b"\x1b[<32;82;12M", settle=0.8)
    case.check("drag is forwarded", "\x1b[<32;" in "".join(host.writes[before:]))

    # Once the child releases DEC mouse reporting, the exact same focused
    # pane becomes selectable without dropping focus or mouse capture.
    host.mouse_reporting = False
    host.cursor_col = len(host.content)
    tui.read_for(1.5)

    mark = len(tui.buffer)
    tui.drag((37, 1), (40, 1))  # "live"
    case.check(
        "active-terminal drag copies the selected text",
        b"\x1b]52;c;bGl2ZQ==\x07" in tui.buffer[mark:],
    )
    case.check(
        "drag selection keeps mouse capture enabled",
        not any(mode in tui.buffer[mark:] for mode in (b"\x1b[?1000l", b"\x1b[?1002l", b"\x1b[?1003l")),
    )
    case.check("copy is acknowledged", "selection copied" in tui.expect("selection copied"))

    # Mouse-up retains the range. Backspace consumes that retained selection
    # as an edit on the live cursor row: move from the end of "live session"
    # to the end of "live", then delete all four selected graphemes.
    before = len(host.writes)
    tui.send(b"\x7f", settle=0.8)
    selection_edit = "".join(host.writes[before:])
    case.check(
        "backspace deletes the retained input selection",
        selection_edit == "\x1b[D" * len(" session") + "\x7f" * len("live"),
        repr(selection_edit),
    )

    # Focus never left: a normal key reaches the same host immediately.
    before = len(host.writes)
    tui.send(b"\x16z", settle=0.8)
    focused_input = "".join(host.writes[before:])
    case.check("selection keeps terminal focus", "z" in focused_input)
    case.check("ctrl+v still belongs to the active terminal", "\x16" in focused_input)

    # Wheel routing follows the child modes, not whether the sidebar happens
    # to own the keyboard. This is the Herdr behavior and keeps a terminal
    # scrollable before/after typing into it.
    tui.send(b"\x1d", settle=0.5)  # ctrl+] -> sidebar focus
    host.mouse_reporting = True
    tui.read_for(1.5)
    before = len(host.writes)
    tui.scroll(79, 9, up=True)
    case.check(
        "mouse-reporting terminal scrolls with sidebar focused",
        "\x1b[<64;43;9M" in "".join(host.writes[before:]),
    )

    host.mouse_reporting = False
    host.alternate_screen = True
    host.mouse_alternate_scroll = True
    host.application_cursor = True
    snapshot_before = host.snapshot_requests
    case.check(
        "alternate-screen modes reach the TUI before scrolling",
        tui.wait_for(lambda: host.snapshot_requests > snapshot_before, timeout=5.0),
    )
    before = len(host.writes)
    tui.scroll(79, 9, up=True)
    case.check(
        "alternate-scroll mode receives application cursor keys",
        "\x1bOA" in "".join(host.writes[before:]),
    )

    # Zero history is not alternate screen. A fresh shell must stay on host
    # scrollback routing and must not receive made-up mouse reports.
    host.alternate_screen = False
    host.mouse_alternate_scroll = False
    host.application_cursor = False
    snapshot_before = host.snapshot_requests
    case.check(
        "fresh-shell modes reach the TUI before scrolling",
        tui.wait_for(lambda: host.snapshot_requests > snapshot_before, timeout=5.0),
    )
    before = len(host.writes)
    tui.scroll(79, 9, up=True)
    case.check("fresh shell wheel sends no synthetic input", len(host.writes) == before)


run("mouse", body)
