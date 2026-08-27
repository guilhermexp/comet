"""Recent archived sessions share the five-row stopped preview; `a` on the
selected project opens its complete archive library on the right — with
search, Restore & Resume (or Restore for unsupported commands), and delete.
The project context menu's "Archived (N)" is the
other way in, and there is no separate archive footer row."""

import sys, os

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from harness import run  # noqa: E402


def body(case):
    home = case.home
    home.project("p", "unpeel", "/tmp")
    home.session("live-a", label="active one", project_id="p", created_at=1_754_400_000_000)
    for session_id, label, created, command in [
        ("arch-a", "archived alpha", 1_754_300_000_000, "claude"),
        ("arch-b", "archived beta", 1_754_200_000_000, "unsupported-shell"),
    ]:
        home.session(
            session_id,
            label=label,
            command=command,
            project_id="p",
            created_at=created,
        )
        home.marker(session_id, "archived.json", {"archived_at": created})

    tui = case.pty(cols=160)
    tui.read_for(3.5)
    screen = tui.screen()

    case.check("no archive row in the session list", "Archive (" not in screen)
    case.check(
        "recent archived sessions remain in the stopped preview",
        "archived alpha" in tui.sidebar() and "archived beta" in tui.sidebar(),
        tui.sidebar()[:240],
    )

    # The footer row is gone; the primary mouse path is the project's
    # context menu, matching desktop.
    tui.click(5, 1, button=2)
    menu = tui.expect("Archived (2)")
    case.check(
        "the project menu offers the archive library",
        "Archived (2)" in menu,
        menu[:240],
    )
    # New session, New group…, Archived (2).
    tui.send("jj\r", settle=0.8)
    library = tui.expect("archived alpha", "archived beta", "Archive ·")
    case.check("the project menu opens the archive library", "archived alpha" in library)
    case.check("library renders on the right", "Archive ·" in library)
    case.check("both archived sessions listed", "archived beta" in library, library[:200])
    case.check(
        "a resumable archived session offers Restore & Resume",
        "Restore & Resume" in library,
        library[:240],
    )

    tui.type("beta")
    tui.expect("search", "archived beta")
    tui.wait_for(
        lambda: "archived beta" in tui.preview_text(0.4)
        and "archived alpha" not in tui.preview_text(0.4),
        timeout=10,
    )
    searched = tui.preview_text()
    case.check("search box appears", "search" in searched)
    case.check(
        "search filters the library",
        "archived beta" in searched and "archived alpha" not in searched,
        searched[:200],
    )

    tui.backspace(4)
    tui.read_for(0.8)
    tui.send("\r", settle=1.2)
    case.check(
        "Restore & Resume clears the shared marker",
        not home.has_marker("arch-a", "archived.json"),
    )
    case.check("resume reports back", "resuming" in tui.expect("resuming"))
    case.check(
        "restored session returns to the list",
        "archived alpha" in tui.expect("archived alpha"),
    )

    # Reopening shows only what is still filed.
    tui.send("a", settle=0.8)
    reopened = tui.expect("Archive ·", "archived beta", "Restore")
    case.check("reopened library lists the remaining session", "archived beta" in reopened)
    case.check(
        "an unsupported archived command offers plain Restore only",
        "Restore & Resume" not in reopened,
        reopened[:240],
    )
    tui.send("\r", settle=1.0)
    case.check(
        "plain Restore keeps the same stopped Session",
        "arch-b" in home.manifests()
        and not home.has_marker("arch-b", "archived.json"),
        str(home.manifests().get("arch-b")),
    )

    # ── the recency stamp: user archives float and linger, sweeps don't ──
    # A sweep-filed marker (stamped: false) does not gain a fresh user-action
    # rank, but a genuinely recent stopped Session still belongs to the
    # shared five-row preview.
    import time as _time

    home.session("swept", label="swept session", project_id="p",
                 created_at=1_754_500_000_000)
    home.marker("swept", "archived.json",
                {"archived_at": int(_time.time() * 1000), "stamped": False})
    tui.send("a", settle=0.8)
    filed = tui.expect("swept session", timeout=12)
    case.check(
        "a recent sweep-filed session remains in the stopped preview",
        "swept session" in tui.sidebar(),
        tui.sidebar()[:200],
    )
    case.check("it still lands in the archive library", "swept session" in filed, filed[:160])
    tui.send("\x1b", settle=0.5)

    # A USER archive from the CLI lingers visibly (stamped: true).
    from harness import run_cli

    home.session("filed-by-me", label="filed by me", project_id="p",
                 created_at=1_754_100_000_000)
    run_cli(home, ["archive", "filed-by-me"])
    lingering = tui.expect("filed by me", timeout=12)
    case.check(
        "a user archive lingers in the visible list",
        "filed by me" in lingering,
        lingering[:200],
    )
    marker = home.read_marker("filed-by-me", "archived.json")
    case.check(
        "and its marker is stamped",
        bool(marker) and marker.get("stamped") is True,
        str(marker),
    )


run("archive", body)
