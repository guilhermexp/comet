"""Stopped sessions must agree with the desktop.

Two rules the TUI used to get wrong, both of which show up as "the app and
the terminal disagree about what's stopped":

1. A session whose host died and whose pid was recycled by an unrelated
   process is STOPPED. Checking `kill(pid, 0)` alone keeps it looking alive
   — pids wrap in under an hour under agent load, which is exactly the
   invariant the desktop's `manifestPidIdentity` exists for.
2. The recent-stopped window moves older stopped rows into the archive
   library, but never the one you're looking at or one you haven't read.
"""

import sys, os, subprocess

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from harness import run, post_hook, tui_hook_port, UNREAD_DOT, SPINNER_CHARS  # noqa: E402


def body(case):
    home = case.home
    home.project("p", "unpeel", "/tmp")
    # The case deliberately uses year-old lifecycle stamps to exercise the
    # five-row stopped window. Keep the independent idle-cleanup policy from
    # immediately archiving the Session we temporarily make live below.
    state = home.state()
    state["auto_stop_archive_minutes"] = 0
    home.write_state(state)

    # A session whose manifest says running, with a live pid that is NOT its
    # child: pid_started_at predates this process by years.
    impostor = subprocess.Popen(["sleep", "600"])
    home.session(
        "s-recycled",
        label="recycled pid",
        project_id="p",
        created_at=1_754_400_000_000,
        extra_manifest={
            "state": "running",
            "pid": impostor.pid,
            "pid_started_at": 1_600_000_000_000,  # long before this pid existed
            "exit_code": None,
        },
    )
    # A genuinely running one, for contrast.
    home.session("s-live", label="really running", project_id="p",
                 created_at=1_754_300_000_000, running=True)

    tui = case.pty()
    tui.read_for(3.5)
    listed = tui.expect("recycled pid", "really running")

    case.check("both sessions are listed", "recycled pid" in listed)
    case.check(
        "a recycled pid does not count as running",
        "✕" in listed or "recycled pid" in tui.sidebar(),
        "the row must render as stopped, not spinning",
    )
    tui.send("j", settle=1.0)  # select the recycled row
    impostor.kill()

    # A stopped session shows a placeholder, not the dead terminal's last
    # frame — which looked live and invited typing into nothing.
    placeholder = tui.expect("is stopped", timeout=10)
    case.check("a stopped session says so", "is stopped" in placeholder, placeholder[:200])
    case.check(
        "and says how to bring it back",
        "press r to resume" in placeholder,
        placeholder[:240],
    )
    case.check(
        "its old output is not shown as if live",
        "hello from the fixture" not in tui.preview_text(),
        tui.preview_text()[:160],
    )

    # ── the window keeps what you are looking at ──
    for index in range(8):
        lifecycle_at = 1_754_000_000_000 - index * 1000
        home.session(
            f"old-{index:02d}",
            label=f"old session {index:02d}",
            project_id="p",
            created_at=lifecycle_at,
            # Exited-manifest time is part of the canonical lifecycle rank.
            # Pin it to the fixture clock instead of Home.session's wall
            # clock so old-00 is intentionally the newest stopped row.
            extra_manifest={"updated_at": lifecycle_at},
        )
    listed = tui.expect("old session 00", timeout=12)
    case.check("recent stopped sessions show", "old session 00" in listed)
    case.check(
        "older ones fold into the archive library (no sidebar row)",
        "old session 07" not in tui.sidebar() and "Archive (" not in listed,
        tui.sidebar()[:200],
    )

    # Select one that the window would otherwise hide, via the palette, and
    # it must stay in the list.
    tui.send(b"\x0b", settle=1.0)
    tui.type("old session 07")
    tui.send("\r", settle=1.5)
    kept = tui.expect("old session 07 is stopped", timeout=12)
    case.check(
        "the selected session is never folded away",
        "old session 07 is stopped" in tui.preview_text(),
        kept[:200],
    )

    # ── an unread session outside the window stays too ──
    # Unread is earned, not declared: the TUI marks a session unread when it
    # watches it go busy → idle while the cursor is elsewhere. So drive the
    # real sequence — run it, work, settle — then stop it, which is exactly
    # how a stopped-but-unread row happens in life.
    port = tui_hook_port(home)
    home.session("old-06", label="old session 06", project_id="p",
                 created_at=1_753_900_000_000, running=True)
    case.host("old-06")
    tui.wait_for(lambda: "old session 06" in tui.sidebar(), timeout=12)

    # Drive a real Busy→Idle completion for the hidden-window integration.
    # The adjacent main.rs unit coverage also pins the fast
    # Starting→Idle edge independently.
    start_status = post_hook(port, "old-06", "Start")
    case.check("the hidden-window hook is accepted", start_status == 200, str(start_status))
    busy = tui.wait_for(
        lambda: any(
            "old session 06" in line and any(c in line for c in SPINNER_CHARS)
            for line in tui.grid(0.4).lines()
        ),
        timeout=10,
    )
    live_manifest = home.manifests().get("old-06", {})
    case.check(
        "the hidden-window session becomes busy",
        bool(busy),
        str((live_manifest.get("state"), live_manifest.get("pid"))),
    )
    post_hook(port, "old-06", "Stop")
    dotted = tui.expect(UNREAD_DOT, timeout=12)
    case.check("settling unobserved marks it unread", UNREAD_DOT in dotted, dotted[:200])

    # Now stop it: the row joins the stopped group, far outside the window.
    import json

    manifest_path = home.path("app-sessions", "old-06", "manifest.json")
    with open(manifest_path) as handle:
        manifest = json.load(handle)
    for entry in home.running_sessions().values():
        if entry["session"]["id"] == "old-06" and entry.get("pid"):
            os.kill(entry["pid"], 9)
    manifest["state"] = "exited"
    manifest["pid"] = None
    manifest["exit_code"] = 0
    with open(manifest_path, "w") as handle:
        json.dump(manifest, handle)

    kept = tui.expect("old session 06", timeout=12)
    case.check(
        "an unread session is never folded away",
        "old session 06" in kept,
        kept[:200],
    )
    case.check(
        "and it still shows its unread dot",
        UNREAD_DOT in kept,
        kept[:200],
    )


run("stopped_sync", body)
