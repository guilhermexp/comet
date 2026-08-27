"""Appended system context is shared state: set it from any frontend,
and whichever one resumes the stopped session applies it.

It is a restart-only launch flag — applied to the relaunch command with
the provider's own flag, then consumed (the new session's command carries
it permanently)."""

import sys, os

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from harness import run, run_cli  # noqa: E402


def body(case):
    home = case.home
    home.project("p", "unpeel", "/tmp")
    # pi supports appended context and its resume arm has no on-disk
    # pre-flight, so the relaunch is fully assertable.
    home.session("s-pi", label="pi convo", project_id="p",
                 command="pi --yolo --append-system-prompt 'first instruction'")

    # ── set from the CLI (any frontend, no app anywhere) ──
    set_result = run_cli(home, ["context", "s-pi", "always answer in haiku"])
    case.check("the CLI sets it", set_result.returncode == 0, set_result.stderr[:160])
    marker = home.read_marker("s-pi", "appended-context.json")
    case.check(
        "it lands in the shared marker",
        bool(marker) and marker.get("context") == "always answer in haiku",
        str(marker),
    )

    # ── a provider that can't take it is refused ──
    home.session("s-cat", label="plain terminal", project_id="p", command="cat")
    refused = run_cli(home, ["context", "s-cat", "nope"])
    case.check(
        "unsupported providers are refused",
        refused.returncode != 0 and "does not support" in refused.stderr,
        refused.stderr[:160],
    )

    # ── the TUI's stopped-session Resume applies and consumes it ──
    tui = case.pty()
    tui.read_for(3.0)
    grid = tui.grid()
    pi_row = next(row for row in range(grid.rows) if "pi convo" in grid.row(row))
    tui.click(5, pi_row, button=2)
    tui.expect("Resume")
    tui.send("\x1b", settle=0.3)
    tui.send("r", settle=1.0)
    new_manifest = tui.wait_for(
        lambda: next(
            (m for sid, m in home.manifests().items()
             if m["session"]["command"].startswith("pi") and sid != "s-pi"),
            None,
        ),
        timeout=25,
    )
    case.check("Resume spawned a replacement", bool(new_manifest))
    if new_manifest:
        command = new_manifest["session"]["command"]
        case.check(
            "the relaunch merges old and new context into one pi flag",
            command.count("--append-system-prompt") == 1
            and "--append-system-prompt 'first instruction\n\nalways answer in haiku'" in command,
            command,
        )
        new_id = new_manifest["session"]["id"]
        case.check(
            "and the pending marker was consumed",
            not home.has_marker(new_id, "appended-context.json"),
            "the new session's command carries it permanently instead",
        )

    # ── the host verb the APP calls returns the same derivation ──
    # (unpeel-host __resume__ is what ResumeCommand.hostRelaunchCommand
    # shells out to — this validates the exact artifact.)
    import subprocess

    from harness import CRATES

    host = os.path.join(CRATES, "target", "debug", "unpeel-host")
    home.session(
        "s-pi3",
        label="pi three",
        project_id="p",
        command="pi --yolo --append-system-prompt='first instruction'",
    )
    home.marker("s-pi3", "provider-session.json", {"provider_session_id": "pv-77"})
    home.marker("s-pi3", "appended-context.json", {"context": "haiku only"})
    result = subprocess.run(
        [host, "__resume__", "s-pi3"],
        capture_output=True, text=True, timeout=20,
        env=dict(os.environ, UNPEEL_HOME=home.root),
    )
    derived = {}
    try:
        derived = __import__("json").loads(result.stdout)
    except ValueError:
        pass
    case.check(
        "unpeel-host __resume__ derives the same command the TUI would",
        result.returncode == 0
        and "--session 'pv-77'" in derived.get("command", "")
        and derived.get("command", "").count("--append-system-prompt") == 1
        and "--append-system-prompt 'first instruction\n\nhaiku only'"
        in derived.get("command", ""),
        result.stdout[:200] + result.stderr[:120],
    )
    fork = subprocess.run(
        [host, "__resume__", "s-pi3", "--fork"],
        capture_output=True, text=True, timeout=20,
        env=dict(os.environ, UNPEEL_HOME=home.root),
    )
    case.check(
        "fork is refused for providers without the primitive",
        fork.returncode != 0 and "no native fork primitive" in fork.stderr,
        fork.stderr[:160],
    )

    # ── clearing works too ──
    home.session("s-pi2", label="pi two", project_id="p", command="pi --yolo")
    run_cli(home, ["context", "s-pi2", "temp"])
    cleared = run_cli(home, ["context", "s-pi2"])
    case.check(
        "no text clears it",
        cleared.returncode == 0 and not home.has_marker("s-pi2", "appended-context.json"),
        cleared.stdout[:120],
    )


run("appended_context", body)
