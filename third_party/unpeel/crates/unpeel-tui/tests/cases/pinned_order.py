"""Pinned sessions reorder by drag too.

They used to sort only by pin time and the desktop's overlay, so a drag
inside the pinned group did nothing and the row snapped back. Pinned rows
now rank by the same shared order every other row uses.

(Pin-time ordering itself comes from the desktop overlay, which a headless
fixture has none of — so this asserts the ordering the TUI controls.)"""

import sys, os, json

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from harness import run  # noqa: E402


def body(case):
    home = case.home
    home.project("p", "unpeel", "/tmp")
    home.session("s-a", label="pinned alpha", project_id="p", created_at=1_754_100_000_000)
    home.session("s-b", label="pinned bravo", project_id="p", created_at=1_754_200_000_000)
    home.session("s-c", label="loose gamma", project_id="p", created_at=1_754_300_000_000)
    home.pin("s-a", pinned_at=1)
    home.pin("s-b", pinned_at=2)

    tui = case.pty()
    tui.read_for(3.0)
    first = tui.sidebar()

    case.check(
        "pinned rows sit above the unpinned ones",
        first.index("pinned alpha") < first.index("loose gamma")
        and first.index("pinned bravo") < first.index("loose gamma"),
        first[:200],
    )
    case.check("both render their star", first.count("⭑") == 2, first[:200])

    top_is_alpha = first.index("pinned alpha") < first.index("pinned bravo")

    # Drag the top pinned row down one, onto the other pinned row.
    tui.drag((4, 2), (4, 3))
    tui.read_for(1.5)
    after = tui.sidebar()

    case.check(
        "a drag reorders within the pinned group",
        (after.index("pinned alpha") < after.index("pinned bravo")) != top_is_alpha,
        f"before top_is_alpha={top_is_alpha}; after={after[:160]}",
    )
    case.check(
        "they stay pinned above the rest",
        max(after.index("pinned alpha"), after.index("pinned bravo"))
        < after.index("loose gamma"),
        after[:200],
    )

    order_path = home.path("session-order.json")
    order = {}
    if os.path.exists(order_path):
        with open(order_path) as handle:
            order = json.load(handle)
    expected_first = "s-a" if top_is_alpha else "s-b"
    case.check(
        "the new order is shared with the app",
        order.get("p", [])[:2] == (["s-b", "s-a"] if top_is_alpha else ["s-a", "s-b"]),
        f"{order} (top was {expected_first})",
    )


run("pinned_order", body)
