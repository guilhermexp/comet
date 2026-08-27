"""The sidebar header is Projects, every session — including ones far
down the list — is reachable through the palette from anywhere, and the
palette lists sessions "All recent" style: by real recency (hook-seed /
read-receipt / created), not creation order. The unfiltered view tiers
like the desktop's activity popover: working sessions under an "active"
caption, everything else under "recent"."""

import json
import sys, os, time

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from harness import run, post_hook, tui_hook_port, SPINNER_CHARS, UNREAD_DOT  # noqa: E402


def body(case):
    home = case.home
    home.project("p", "unpeel", "/tmp")
    # This case supplies deliberately old recency stamps. Keep the separate
    # one-day idle cleanup policy from archiving s4 after it is deselected.
    state = home.state()
    state["auto_stop_archive_minutes"] = 0
    with open(home.path("app-state.json"), "w") as handle:
        json.dump(state, handle, indent=2)
    home.session("s1", label="alpha session", project_id="p", created_at=1_754_100_000_000)
    home.session("s2", label="beta session", project_id="p", created_at=1_754_200_000_000)
    home.session("s3", label="gamma notes", project_id="p", created_at=1_754_300_000_000)
    home.session("s4", label="delta build", project_id="p", running=True)
    home.project("q", "otherproj", "/tmp")
    home.session("s5", label="omega elsewhere", project_id="q", created_at=1_754_000_000_000)
    # Recency inverts creation order: the OLDEST-created session had the
    # newest turn. Seed mtimes sit in the near future so the TUI's own
    # read receipt on the auto-selected session can't outrank them.
    now = time.time()
    for sid, offset in (("s1", 3600), ("s2", 1800), ("s3", 900)):
        home.settle(sid)
        seed = home.path("app-sessions", sid, "last-hook-event.json")
        os.utime(seed, (now + offset, now + offset))

    tui = case.pty()
    tui.read_for(3.0)
    screen = tui.screen()

    case.check("the sidebar is titled Projects", "Projects" in screen)
    case.check(
        "the menu is the only bottom-border chrome",
        "menu" in tui.sidebar() and "(^K)" not in tui.sidebar(),
        tui.sidebar()[-160:],
    )

    tui.send(b"\x0b", settle=1.0)
    palette = tui.expect("command palette")
    case.check("ctrl+K opens it", "command palette" in palette)
    case.check(
        "every session is listed",
        all(name in palette for name in ("alpha session", "beta session", "gamma notes")),
        palette[:240],
    )
    # The palette rows sit below the sidebar copies of the same labels in
    # the row-major frame, so each label's LAST occurrence is the palette's —
    # and those must run in recency order, not the sidebar's newest-created.
    positions = [
        palette.rfind(name)
        for name in ("alpha session", "beta session", "gamma notes")
    ]
    case.check(
        "sessions rank by recency, not creation order",
        -1 not in positions and positions == sorted(positions),
        str(positions),
    )

    tui.type("alpha")
    filtered = tui.expect("alpha")
    case.check("the query is echoed", "alpha" in filtered)

    tui.send("\r", settle=1.5)
    case.check("selecting jumps to that session", "alpha session" in tui.expect("alpha session"))

    tui.send("/", settle=1.0)
    case.check("slash opens the palette too", "command palette" in tui.expect("command palette"))
    tui.send("\x1b", settle=0.6)

    # Tiered unfiltered view: a hooked-busy session moves under an "active"
    # caption, the current project's sessions follow, and a "projects"
    # section switches project. Idle sessions in OTHER projects stay out of
    # the unfiltered list (omega's only screen copy is its sidebar row).
    port = tui_hook_port(home)
    post_hook(port, "s4", "Start")
    tui.wait_for(lambda: any(c in tui.frame(0.5) for c in SPINNER_CHARS), timeout=10)
    tui.send(b"\x0b", settle=1.0)
    tiers = tui.expect("active")
    order = [
        tiers.find("active"),
        tiers.rfind("delta build"),
        tiers.rfind("alpha session"),
        tiers.find("projects"),
    ]
    case.check(
        "active tier tops the list, current project follows, projects switch",
        -1 not in order and order == sorted(order),
        str(order),
    )
    case.check(
        "another project's idle session stays out of the unfiltered view",
        tiers.count("omega") == 1 and "otherproj" in tiers,
        f"omega x{tiers.count('omega')}",
    )
    tui.type("omega")
    found = tui.expect("omega elsewhere")
    case.check(
        "typing still reaches every project's sessions",
        found.count("omega") >= 3,  # sidebar row + query echo + palette row
        f"omega x{found.count('omega')}",
    )
    tui.send("\x1b", settle=0.6)

    # Settling unobserved moves the job to the unread "recent" tier. The
    # durable seed rewrite mirrors what a real provider hook script does at
    # Stop — without it the startup read receipt (s4 is the only running
    # session, so it is the auto-selection) outranks the settle.
    post_hook(port, "s4", "Stop")
    home.settle("s4")
    tui.wait_for(lambda: UNREAD_DOT in tui.frame(0.5), timeout=12)
    tui.send(b"\x0b", settle=1.0)
    settled = tui.expect("recent")
    case.check(
        "a job that settled unobserved tiers under recent",
        settled.find("active") == -1
        and -1 != settled.find("recent") < settled.rfind("delta build"),
        f"active={settled.find('active')} recent={settled.find('recent')} delta={settled.rfind('delta build')}",
    )
    tui.send("\x1b", settle=0.6)


run("recents", body)
