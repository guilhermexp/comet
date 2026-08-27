"""The project tree: a permanent way to add one, and drag to reorder.

Project order is a shared file like session order, so a drag here shows up
in the desktop app too."""

import sys, os, json

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from harness import run  # noqa: E402


def body(case):
    home = case.home
    home.project("a", "alpha", "/tmp")
    home.project("b", "bravo", "/tmp")
    home.project("c", "charlie", "/tmp")
    home.session("s-a", label="in alpha", project_id="a")
    home.session("s-b", label="in bravo", project_id="b")
    home.session("s-c", label="in charlie", project_id="c")

    tui = case.pty()
    tui.read_for(3.0)
    first = tui.sidebar()

    case.check(
        "projects render in order",
        first.index("alpha") < first.index("bravo") < first.index("charlie"),
        first[:200],
    )
    case.check("the tree offers a way to add one", "+ Add project" in first, first[-160:])
    case.check(
        "and it is the last row",
        first.index("+ Add project") > first.index("charlie"),
        first[-160:],
    )

    # ⏎ on the row opens the same dialog the + key does.
    for _ in range(20):
        tui.send("j", settle=0.08)
    tui.read_for(0.6)
    tui.send("\r", settle=1.2)
    dialog = tui.expect("add a project")
    case.check("it opens the add-project dialog", "add a project" in dialog, dialog[:160])
    tui.send("\x1b", settle=0.8)

    # Mid-drag the tree must already show the result — the whole group
    # (header, its rows) rendered where it would land, not a hint on drop.
    # Rows: 0 ▾alpha 1 in alpha 2 ▾bravo 3 in bravo 4 ▾charlie
    tui.send(b"\x1b[<0;5;2M", settle=0.4)   # press on the alpha header
    tui.send(b"\x1b[<32;5;6M", settle=0.6)  # drag down onto charlie
    mid = tui.sidebar()
    case.check(
        "the carried project shows its exact landing position",
        mid.index("bravo") < mid.index("alpha") < mid.index("charlie"),
        mid[:240],
    )
    case.check(
        "its whole group travels with it",
        mid.index("alpha") < mid.index("in alpha"),
        mid[:240],
    )
    tui.send(b"\x1b[<0;5;6m", settle=0.6)   # release
    tui.read_for(1.5)
    after = tui.sidebar()
    case.check(
        "release keeps the carried project's shown position",
        after.index("bravo") < after.index("alpha") < after.index("charlie"),
        after[:240],
    )
    case.check(
        "its sessions travel with it",
        after.index("alpha") < after.index("in alpha"),
        after[:240],
    )
    case.check(
        "and dragging it did not fold it",
        "▸ alpha" not in after,
        "grabbing a header used to collapse it on mouse-down",
    )

    order_path = home.path("project-order.json")
    case.check("the order is shared with the app", os.path.exists(order_path))
    if os.path.exists(order_path):
        with open(order_path) as handle:
            order = json.load(handle)
        case.check(
            "it records the exact previewed project order",
            order == ["b", "a", "c"],
            str(order),
        )


run("project_order", body)
