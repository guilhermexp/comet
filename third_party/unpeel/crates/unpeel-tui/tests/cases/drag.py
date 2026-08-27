"""Drag-to-reorder in the sidebar, and the shared order file that makes a
drag here show up in the desktop app (and the reverse).

Sessions belong to their group: a drag reorders within a group and can never
move a session to another group."""

import sys, os, json

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from harness import run  # noqa: E402


SESSION_IDS = {"alpha": "s-a", "bravo": "s-b", "gamma": "s-c"}


def rendered_sessions(tui):
    grid = tui.grid()
    width = grid.sidebar_width()
    found = []
    for row, line in enumerate(grid.lines()):
        sidebar = line[:width]
        for name in SESSION_IDS:
            if name in sidebar:
                found.append((row, name))
    return sorted(found)


def body(case):
    home = case.home
    home.project("p", "unpeel", "/tmp")
    home.project("q", "other", "/tmp")
    home.session("s-a", label="alpha", project_id="p", created_at=1_754_100_000_000)
    home.session("s-b", label="bravo", project_id="p", created_at=1_754_200_000_000)
    home.session("s-c", label="gamma", project_id="p", created_at=1_754_300_000_000)
    home.session("s-far", label="delta", project_id="q")

    tui = case.pty()
    tui.read_for(3.5)
    initial_rows = rendered_sessions(tui)
    initial = [name for _, name in initial_rows]

    case.check(
        "all lifecycle-ranked rows render",
        sorted(initial) == sorted(SESSION_IDS),
        str(initial),
    )
    dragged = initial[0]
    target = initial[-1]
    dragged_row = initial_rows[0][0]
    target_row = initial_rows[-1][0]

    # Mid-drag the sidebar must show the result, not a tint on whatever row
    # the cursor happens to be over: the carried block renders in the place
    # it would land, so you can see where you're putting it.
    tui.send(f"\x1b[<0;5;{dragged_row + 1}M".encode(), settle=0.4)
    tui.send(f"\x1b[<32;5;{target_row + 1}M".encode(), settle=0.6)
    mid = tui.sidebar()
    case.check(
        "the carried row moves under the cursor",
        mid.index(dragged) > mid.index(target),
        mid[:200],
    )
    tui.send(f"\x1b[<0;5;{target_row + 1}m".encode(), settle=0.6)
    # The drop must land where the preview showed it, with no bounce: the
    # model is rebuilt on commit rather than on the next poll tick, so there
    # is never a frame showing the old order.
    settled = tui.sidebar()
    case.check(
        "the drop lands without bouncing back",
        settled.index(dragged) > settled.index(target),
        settled[:200],
    )
    tui.read_for(1.5)

    order_path = home.path("session-order.json")
    order = {}
    if os.path.exists(order_path):
        with open(order_path) as handle:
            order = json.load(handle)
    ids = order.get("p", [])
    case.check("order persists to the shared file", len(ids) == 3, str(order))
    case.check(
        "the dragged root moved",
        bool(ids) and ids[-1] == SESSION_IDS[dragged],
        str(ids),
    )
    case.check(
        "a session never crosses projects",
        "s-far" not in ids and order.get("q") in (None, []),
        str(order),
    )

    after_order = [name for _, name in rendered_sessions(tui)]
    case.check(
        "the model reflects the new order",
        after_order == initial[1:] + [dragged],
        str(after_order),
    )
    # An order written by the desktop app is honoured on the next scan.
    external_order = list(reversed(initial))
    with open(order_path, "w") as handle:
        json.dump({"p": [SESSION_IDS[name] for name in external_order]}, handle)
    adopted = tui.wait_for(
        lambda: [name for _, name in rendered_sessions(tui)] == external_order,
        timeout=12,
    )
    case.check(
        "an order written elsewhere is adopted",
        bool(adopted),
        str([name for _, name in rendered_sessions(tui)]),
    )


run("drag", body)
