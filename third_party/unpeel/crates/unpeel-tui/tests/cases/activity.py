"""The activity engine: the hook bus, the spinner it drives, and the port
registry that lets several Unpeel instances coexist.

The 404-for-unknown-session rule is what stops one instance from swallowing
another's events, so it is asserted here rather than assumed."""

import json
import sys, os

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from harness import run, post_hook, tui_hook_port, SPINNER_CHARS  # noqa: E402


def body(case):
    home = case.home
    home.project("p", "unpeel", "/tmp")
    home.session("s-live", label="busy session", project_id="p", running=True)

    app = case.app()  # a mock desktop already holds a port
    tui = case.pty()
    tui.read_for(3.0)

    ports = home.ports()
    case.check("both instances registered", len(ports) == 2 and app.port in ports, str(ports))
    port = tui_hook_port(home, exclude=(app.port,))
    case.check("tui registered its own port", port is not None)

    case.check("hook 200 for a known session", post_hook(port, "s-live", "Start") == 200)
    case.check(
        "hook 404 for a foreign session",
        post_hook(port, "not-ours", "Start") == 404,
        "a foreign instance must not swallow events",
    )

    busy = tui.wait_for(
        lambda: any(c in tui.frame(0.5) for c in SPINNER_CHARS), timeout=10
    )
    case.check("spinner runs after Start", busy)

    post_hook(port, "s-live", "Stop")
    settled = tui.wait_for(
        lambda: not any(c in tui.frame(0.5) for c in SPINNER_CHARS), timeout=10
    )
    case.check("spinner clears after Stop", settled)

    # Hook-capable sessions are hook-owned: raw output must not flip them
    # back to busy once the latch is set.
    with open(home.path("app-sessions", "s-live", "output.bin"), "a") as handle:
        handle.write("noise the agent printed after finishing\r\n")
    tui.read_for(2.5)
    case.check(
        "output alone does not re-arm the spinner",
        not any(c in tui.frame(0.6) for c in SPINNER_CHARS),
    )

    # An idle-animating TUI (grok-style): output.bin keeps growing, but the
    # host's parsed-screen stamp stands still — the session must stay idle.
    home.session("s-anim", label="animating idle", project_id="p", running=True,
                 command="top", extra_manifest={"screen_changed_at": 1_754_000_000_000})
    tui.wait_for(lambda: "animating idle" in tui.sidebar(), timeout=12)
    for _ in range(3):
        with open(home.path("app-sessions", "s-anim", "output.bin"), "a") as handle:
            handle.write("identical repaint frame " * 40 + "\r\n")
        tui.read_for(1.0)
    case.check(
        "growing output with a still screen stamp stays idle",
        not any(c in tui.frame(0.6) for c in SPINNER_CHARS),
    )
    # A real screen change advances the stamp: busy resumes.
    manifest_path = home.path("app-sessions", "s-anim", "manifest.json")
    with open(manifest_path) as handle:
        manifest = json.load(handle)
    manifest["screen_changed_at"] = 1_754_000_600_000
    with open(manifest_path, "w") as handle:
        json.dump(manifest, handle)
    case.check(
        "an advancing screen stamp re-arms busy",
        tui.wait_for(
            lambda: any(c in tui.frame(0.5) for c in SPINNER_CHARS), timeout=10
        ),
    )

    # Attention (a permission prompt) is distinct from busy. The event name
    # is PermissionRequest in both frontends; plain "Notification" is
    # deliberately ignored, matching SessionActivity.swift.
    post_hook(port, "s-live", "PermissionRequest")
    attention = tui.wait_for(lambda: "◆" in tui.frame(0.5), timeout=10)
    case.check("notification raises attention", attention)

    tui.send("q")
    case.check("exits cleanly", tui.exited())
    case.check(
        "port unregistered on exit",
        home.ports() == [app.port],
        str(home.ports()),
    )


run("activity", body)
