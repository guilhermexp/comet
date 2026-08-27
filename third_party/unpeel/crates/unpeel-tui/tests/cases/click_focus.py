"""A running Session opens active for input on its first sidebar click."""

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from harness import run  # noqa: E402


def body(case):
    home = case.home
    home.project("p", "unpeel", "/tmp")
    state = home.state()
    state["auto_stop_archive_minutes"] = 0
    home.write_state(state)
    hosts = {}
    for session_id, label in (
        ("live-1", "first candidate"),
        ("live-2", "second candidate"),
    ):
        home.session(session_id, label=label, project_id="p", running=True)
        hosts[label] = case.host(session_id, content=f"{label} terminal")

    tui = case.pty()
    tui.expect("first candidate", "second candidate", timeout=10)

    def visible_session():
        preview = tui.preview_text(0.3)
        return next(
            (label for label in hosts if f"{label} terminal" in preview),
            None,
        )

    selected_label = tui.wait_for(visible_session, timeout=10)
    case.check(
        "the fixture identifies the initially selected Session",
        selected_label is not None,
        (tui.all_text()[-1000:] + "\n" + tui.preview_text()[:200]).strip(),
    )
    if selected_label is None:
        return

    grid = tui.grid()
    sidebar_width = grid.sidebar_width()
    session_rows = {
        label: row
        for row in range(grid.rows)
        for label in hosts
        if label in grid.row(row)[:sidebar_width]
    }
    # Click the other Session so selection and input focus must change in the
    # same mouse event; a stale focus would send the sentinel to the old PTY.
    label = next(label for label in hosts if label != selected_label)
    session_row = session_rows[label]

    tui.click(5, session_row)
    tui.send("z", settle=0.5)
    delivered = tui.wait_for(lambda: "z" in hosts[label].written(), timeout=5)
    case.check(
        "clicking another running Session selects and focuses it immediately",
        bool(delivered)
        and all("z" not in host.written() for name, host in hosts.items() if name != label),
        repr(
            {
                name: {"writes": host.written(), "resizes": host.resizes}
                for name, host in hosts.items()
            }
        )
        + "\n"
        + tui.grid().text()[:400],
    )

    grid = tui.grid()
    sidebar_width = grid.sidebar_width()
    rows = {
        name: row
        for row in range(grid.rows)
        for name in hosts
        if name in grid.row(row)[:sidebar_width]
    }
    tui.drag((5, rows[label]), (5, rows[selected_label]))
    before = {name: host.written() for name, host in hosts.items()}
    tui.send("j", settle=0.5)
    case.check(
        "dragging a running Session keeps input in the sidebar",
        all(host.written() == before[name] for name, host in hosts.items()),
        repr({name: host.written() for name, host in hosts.items()}),
    )


run("click-focus", body)
