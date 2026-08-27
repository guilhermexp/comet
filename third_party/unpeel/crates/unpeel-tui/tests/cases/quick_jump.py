"""^1…^9 jump between sessions inside a project, mirroring the desktop's
⌘1-9, and holding ctrl turns the age column into the jump map.

Both need the kitty keyboard protocol: the legacy encoding sends a bare
"1" for ctrl+1 and never reports a modifier being held. The harness drives
the protocol's escape sequences directly, which is exactly what a terminal
that supports it (Ghostty) sends."""

import sys, os

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from harness import run  # noqa: E402

# kitty keyboard protocol: CSI unicode ; modifiers : event-type u
# 5 = ctrl (1 + 4). Event type 1 = press, 3 = release.
def ctrl_digit(n):
    return f"\x1b[{ord(str(n))};5u".encode()


def rendered_sessions(tui):
    grid = tui.grid()
    width = grid.sidebar_width()
    ordered = []
    for row in grid.lines():
        line = row[:width]
        for index in range(4):
            label = f"session {index}"
            if label in line:
                ordered.append(label)
    return ordered


def body(case):
    home = case.home
    home.project("p", "unpeel", "/tmp")
    home.project("q", "other", "/tmp")
    for index in range(4):
        home.session(
            f"s-{index}",
            label=f"session {index}",
            project_id="p",
            created_at=1_754_400_000_000 - index * 1000,
            output=f"content of session {index}\r\n",
        )
    home.session("far", label="other project session", project_id="q",
                 output="content of far\r\n")

    tui = case.pty()
    tui.read_for(3.0)

    case.check(
        "the age column is left alone",
        "^1" not in tui.sidebar(),
        "hold-to-preview needs a kitty flag that breaks shifted keys — the\n"
        "binding is documented in the (?) overlay instead",
    )

    # Exited-manifest timestamps are lifecycle events, so wall-clock fixture
    # writes can legitimately reorder these old created_at values. Assert the
    # actual contract: the digit addresses the Nth rendered row.
    ordered = rendered_sessions(tui)
    case.check("four project sessions render", len(ordered) == 4, str(ordered))
    third = ordered[2]
    first = ordered[0]

    # The jump itself.
    tui.send(ctrl_digit(3), settle=1.2)
    case.check(
        "^3 selects the third session",
        f"{third} is stopped" in tui.expect(f"{third} is stopped"),
        tui.preview_text()[:160],
    )
    tui.send(ctrl_digit(1), settle=1.2)
    case.check(
        "^1 goes back to the first",
        f"{first} is stopped" in tui.expect(f"{first} is stopped"),
        tui.preview_text()[:160],
    )
    # A number with nothing behind it does nothing.
    tui.send(ctrl_digit(9), settle=1.0)
    case.check(
        "an unused number is a no-op",
        f"{first} is stopped" in tui.preview_text(),
        tui.preview_text()[:160],
    )


    tui.send("?", settle=1.0)
    case.check(
        "the overlay teaches the binding",
        "ctrl+1" in tui.expect("ctrl+1"),
    )


run("quick_jump", body)
