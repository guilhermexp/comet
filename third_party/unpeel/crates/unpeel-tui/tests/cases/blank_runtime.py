"""A CLI started inside a blank Terminal becomes Host-visible runtime state.

The stored launch remains the blank shell: observing `claude` enriches the
live session for sidebar clients, but must not turn a later restart into an
implicit Claude launch. Returning to the shell clears only that observation
and leaves the hosted Terminal alive.
"""

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from harness import run  # noqa: E402


def current_observation(manifest):
    runtime = (manifest or {}).get("runtime") or {}
    return runtime.get("currentObservation")


def session_debug(home):
    details = home.manifests()
    for session_id, manifest in details.items():
        try:
            with open(home.path("app-sessions", session_id, "output.bin"), "rb") as handle:
                manifest["_testOutput"] = handle.read(2_000).decode("utf-8", "replace")
        except OSError:
            pass
    return details


def body(case):
    home = case.home
    project = home.path("project")
    fake_bin = home.path("fake-bin")
    fixture_shell = home.path("fixture-shell")
    os.makedirs(project, exist_ok=True)
    os.makedirs(fake_bin, exist_ok=True)
    home.project("p", "unpeel", project)
    # Give keyboard `n` a selected project/cwd to inherit. The fixture is
    # stopped; only the blank Terminal created below is a live session.
    home.session(
        "seed",
        label="starting point",
        command="true",
        project_id="p",
        cwd=project,
    )

    # A real executable with a deliberately fake provider basename gives the
    # process observer an unambiguous foreground leader without depending on
    # a provider install, network access, or a shell-script interpreter.
    fake_claude = os.path.join(fake_bin, "claude")
    # Symlinking keeps macOS' sealed-system executable at its signed path;
    # copying the Mach-O into a fixture directory makes AMFI kill it before
    # the observer can see the foreground job.
    os.symlink("/bin/sleep", fake_claude)

    # The Host invokes a blank shell with login/interactive flags. A script
    # receives those as ordinary arguments, then replaces itself with the
    # platform baseline shell attached to the same PTY/session leader.
    with open(fixture_shell, "w") as handle:
        handle.write("#!/bin/sh\nexec /bin/sh\n")
    os.chmod(fixture_shell, 0o755)

    # Keep the fixture independent of the developer's login-shell plugins.
    # `/bin/sh` is the portable baseline on both supported Host platforms and
    # exercises the same blank interactive-shell launch path.
    tui = case.pty(env={"SHELL": fixture_shell})
    tui.read_for(3.0)

    # Create the built-in blank Terminal row, not a command preset.
    tui.send("n", settle=1.0)
    picker = tui.expect("Terminal")
    case.check("the blank Terminal choice is offered", "Terminal" in picker, picker[:200])
    tui.send("\r", settle=1.0)

    running = tui.wait_for(
        lambda: home.running_sessions() if len(home.running_sessions()) == 1 else None,
        timeout=25,
    )
    case.check("a blank Terminal starts", bool(running), str(session_debug(home)))
    if not running:
        return

    session_id = next(iter(running))
    initial = running[session_id]
    case.check(
        "the stored launch command is blank",
        initial["session"].get("command") == "",
        repr(initial["session"].get("command")),
    )

    # The newly spawned session is selected. Enter terminal focus, add the
    # fixture directory to this shell's PATH, then type exactly the provider
    # command a user would type in an ordinary blank terminal.
    tui.expect("Terminal", timeout=10)
    tui.send("\r", settle=0.8)
    tui.type(f"export PATH={fake_bin}:$PATH", per_char=0.002, settle=0.2)
    tui.send("\r", settle=0.6)
    tui.type("claude 600", per_char=0.01, settle=0.2)
    tui.send("\r", settle=0.4)

    observed = tui.wait_for(
        lambda: (
            manifest
            if (manifest := home.manifests().get(session_id))
            and (current_observation(manifest) or {}).get("id") == "claude"
            else None
        ),
        timeout=20,
    )
    case.check(
        "the Host observes Claude inside the blank Terminal",
        bool(observed),
        str(session_debug(home).get(session_id)),
    )
    if not observed:
        return

    observation = current_observation(observed)
    argv = observation.get("argv") or []
    case.check(
        "the observation identifies the foreground executable",
        observation.get("id") == "claude"
        and observation.get("pid", 0) > 1
        and argv
        and os.path.basename(argv[0]) == "claude",
        str(observation),
    )
    case.check(
        "runtime detection does not rewrite the launch",
        observed["session"].get("command") == "",
        repr(observed["session"].get("command")),
    )

    # Detach to the sidebar while the observed Claude process still owns the
    # blank Terminal. Passive observation must not grant a relaunch recipe or
    # let the lifecycle shortcut replace the whole terminal.
    observed_pid = observation.get("pid")
    tui.send(b"\x1d", settle=0.4)  # Ctrl+] detaches terminal focus.
    grid = tui.grid()
    session_row = next(
        row for row in range(grid.rows) if observed["session"]["label"] in grid.row(row)
    )
    tui.click(5, session_row, button=2)
    menu = tui.expect("Copy transcript")
    case.check(
        "a passively observed agent has no managed lifecycle action",
        "Resume Agent" not in menu and "Restart Agent" not in menu,
        menu[:240],
    )
    tui.send("\x1b", settle=0.3)
    tui.send("r", settle=0.8)
    after_denied = home.manifests().get(session_id) or {}
    case.check(
        "the lifecycle shortcut cannot restart a passively observed agent",
        after_denied.get("state") == "running"
        and (current_observation(after_denied) or {}).get("pid") == observed_pid,
        str(after_denied),
    )

    # Return focus before interrupting the fake agent.
    tui.send("\r", settle=0.4)

    # Ctrl+C belongs to the focused terminal. It stops the fake agent and
    # returns to the same shell; the observer should clear after its bounded
    # hysteresis without marking the hosted session exited.
    tui.send(b"\x03", settle=0.4)
    cleared = tui.wait_for(
        lambda: (
            manifest
            if (manifest := home.manifests().get(session_id))
            and manifest.get("state") == "running"
            and current_observation(manifest) is None
            else None
        ),
        timeout=20,
    )
    case.check(
        "returning to the shell clears the active runtime",
        bool(cleared),
        str(home.manifests().get(session_id)),
    )
    case.check(
        "the blank Terminal stays live",
        bool(cleared)
        and cleared.get("state") == "running"
        and cleared["session"].get("command") == "",
        str(cleared),
    )


run("blank_runtime", body)
