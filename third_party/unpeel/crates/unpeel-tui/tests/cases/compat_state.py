"""Upgrade safety for people already running Unpeel.

The shared files are a contract across app versions AND across frontends. A
user who installs a newer `unpeel` beside an older desktop app (or updates
the app while sessions from the previous version are on disk) must lose
nothing. Each check here corresponds to a way that could go wrong.
"""

import sys, os, json

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from harness import run, run_cli  # noqa: E402


def body(case):
    home = case.home

    # ── a state file as a shipped desktop writes it, including keys the
    #    Rust side has never modelled ──
    desktop_state = {
        "projects": [{"id": "p", "name": "unpeel", "path": "/tmp"}],
        "active_project_id": "p",
        "presets": [
            {"id": "c", "label": "claude", "command": "claude", "project_id": None,
             "enabled": True, "quick_launch": True}
        ],
        "active_tabs": {"p": "s-legacy"},
        "pinned_sessions": {
            "p": [
                {"key": "session:s-legacy", "project_id": "p",
                 "session_id": "s-legacy", "pinned_at": 1}
            ]
        },
        "theme": "midnight",
        "browser_default_access": "on",
        "a_key_from_a_future_version": {"nested": [1, 2, 3]},
    }
    with open(home.path("app-state.json"), "w") as handle:
        json.dump(desktop_state, handle, indent=2)

    # ── a session dir as an OLD host wrote it: none of the fields added
    #    since (pid_started_at, host_protocol_version, mcp flags…) ──
    legacy_dir = home.path("app-sessions", "s-legacy")
    os.makedirs(legacy_dir, exist_ok=True)
    with open(os.path.join(legacy_dir, "manifest.json"), "w") as handle:
        json.dump(
            {
                "session": {
                    "id": "s-legacy",
                    "project_id": "p",
                    "label": "legacy session",
                    "command": "claude",
                    "created_at": 1_700_000_000_000,
                },
                "cwd": "/tmp",
                "state": "exited",
                "pid": None,
                "exit_code": 0,
            },
            handle,
        )
    with open(os.path.join(legacy_dir, "output.bin"), "w") as handle:
        handle.write("legacy output\r\n")

    # ── a session dir from a FUTURE version: fields we don't know yet ──
    future_dir = home.path("app-sessions", "s-future")
    os.makedirs(future_dir, exist_ok=True)
    with open(os.path.join(future_dir, "manifest.json"), "w") as handle:
        json.dump(
            {
                "session": {
                    "id": "s-future",
                    "project_id": "p",
                    "label": "future session",
                    "command": "claude",
                    "created_at": 1_800_000_000_000,
                    "something_new": "value",
                },
                "cwd": "/tmp",
                "state": "exited",
                "pid": None,
                "exit_code": 0,
                "host_protocol_version": 99,
                "a_field_from_later": True,
            },
            handle,
        )
    with open(os.path.join(future_dir, "output.bin"), "w") as handle:
        handle.write("future output\r\n")

    tui = case.pty()
    tui.read_for(3.5)
    listed = tui.expect("legacy session", "future session")

    case.check("a legacy session still lists", "legacy session" in listed, listed[:200])
    case.check("a future session still lists", "future session" in listed, listed[:200])
    case.check(
        "the project from the app is used",
        "unpeel" in tui.sidebar(),
        tui.sidebar()[:200],
    )

    # ── the TUI writes something, then everything the app owns must survive ──
    tui.send("j", settle=0.8)
    tui.send("e", settle=0.8)
    tui.type("renamed by tui")
    tui.send("\r", settle=1.5)
    tui.send("+", settle=0.8)
    tui.send("\x1b", settle=0.5)
    tui.send("q", settle=1.0)
    tui.exited()

    after = home.state()
    case.check("unmodelled keys survive a TUI run",
               after.get("a_key_from_a_future_version", {}).get("nested") == [1, 2, 3],
               str(after.keys()))
    case.check("theme survives", after.get("theme") == "midnight")
    case.check(
        "pins survive",
        after.get("pinned_sessions") == desktop_state["pinned_sessions"],
        str(after.get("pinned_sessions")),
    )
    case.check("active tabs survive", after.get("active_tabs") == {"p": "s-legacy"})
    case.check("presets survive", after.get("presets") == desktop_state["presets"])
    case.check("the project list survives", after.get("projects") == desktop_state["projects"])

    # ── the CLI must be just as careful ──
    run_cli(home, ["add", home.path("a-folder")]) if os.makedirs(
        home.path("a-folder"), exist_ok=True
    ) is None else None
    after_cli = home.state()
    case.check(
        "the CLI preserves unmodelled keys too",
        after_cli.get("theme") == "midnight"
        and after_cli.get("a_key_from_a_future_version") is not None,
        str(after_cli.keys()),
    )

    # ── a field whose SHAPE we don't recognise must not cost the document ──
    # `pinned_sessions` has already changed shape once in this product's
    # life. If a future app version changes another field, an older `unpeel`
    # must still find the user's projects rather than falling back to
    # `cwd:` buckets and looking like their setup vanished.
    future = dict(desktop_state)
    future["pinned_sessions"] = "a shape from the future"
    future["projects"] = [{"id": "p", "name": "unpeel", "path": "/tmp"}]
    with open(home.path("app-state.json"), "w") as handle:
        json.dump(future, handle, indent=2)
    survivor = case.pty()
    survivor.read_for(3.5)
    case.check(
        "an unreadable field costs only that field",
        "unpeel" in survivor.sidebar(),
        survivor.sidebar()[:200],
    )
    survivor.send("q", settle=1.0)
    survivor.exited()

    # ── a corrupt state file must never be silently replaced ──
    with open(home.path("app-state.json"), "w") as handle:
        handle.write("{ half a write")
    result = run_cli(home, ["add", home.path("another-folder")])
    with open(home.path("app-state.json")) as handle:
        preserved = handle.read()
    case.check(
        "a corrupt state file is not overwritten",
        preserved == "{ half a write",
        preserved[:80],
    )
    case.check(
        "and the failure is reported",
        result.returncode != 0,
        f"rc={result.returncode}",
    )


run("compat_state", body)
