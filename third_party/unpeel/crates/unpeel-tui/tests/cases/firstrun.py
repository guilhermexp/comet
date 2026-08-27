"""First run on a machine that has never been configured: offer the CLIs
that are actually installed, ordered by what the user actually runs, plus
projects inferred from where their existing sessions ran."""

import sys, os

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from harness import run  # noqa: E402


def body(case):
    home = case.home
    # No projects and no presets — the definition of "never configured".
    home.write_state({})
    # Existing sessions are the usage signal. Two ran in the repo, one in
    # a directory that must never be suggested.
    repo = os.path.realpath(os.path.join(os.path.dirname(__file__), "..", "..", ".."))
    home.session("s1", label="one", command="claude", project_id="", cwd=repo)
    home.session("s2", label="two", command="claude", project_id="", cwd=repo)
    home.session("s3", label="three", command="codex", project_id="", cwd="/tmp")

    # Seeding only offers CLIs that are actually installed, so install stubs:
    # the case must behave the same on a bare Linux box (no real agent CLIs)
    # as on a dev Mac that happens to have them.
    stub_bin = os.path.join(home.root, "stub-bin")
    os.makedirs(stub_bin, exist_ok=True)
    for tool in ("claude", "codex"):
        stub = os.path.join(stub_bin, tool)
        with open(stub, "w") as f:
            f.write("#!/bin/sh\nexit 0\n")
        os.chmod(stub, 0o755)

    tui = case.pty(env={"PATH": stub_bin + os.pathsep + os.environ.get("PATH", "")})
    tui.read_for(3.5)
    welcome = tui.expect("welcome to unpeel")

    case.check("a welcome screen is shown", "welcome to unpeel" in welcome, welcome[:200])
    case.check("installed CLIs are offered", "claude" in welcome, welcome[:240])
    case.check(
        "projects are suggested from real sessions",
        "unpeel" in welcome,
        welcome[:240],
    )
    case.check(
        "throwaway directories are not suggested",
        "[x] tmp" not in welcome,
        "a session's cwd of /tmp is not a project",
    )

    tui.send("\r", settle=2.0)
    state = home.state()
    case.check("presets are seeded", len(state.get("presets", [])) > 0, str(state.get("presets")))
    case.check("projects are seeded", len(state.get("projects", [])) > 0)
    case.check(
        "seeded presets have the shared shape",
        all(
            key in state["presets"][0]
            for key in ("id", "label", "command", "enabled", "quick_launch")
        ),
        str(state["presets"][0]),
    )
    case.check(
        "presets are ordered by what the user runs",
        state["presets"][0]["command"].startswith("claude"),
        str([p["command"] for p in state["presets"]]),
    )
    case.check("setup confirms", "setup" in tui.expect("set up").replace(" ", ""))

    # Running again must not re-seed or duplicate anything.
    before = home.state()
    tui.send("q", settle=1.0)
    tui.exited()
    second = case.pty()
    second.read_for(3.0)
    case.check(
        "a configured install goes straight to the sidebar",
        "welcome to unpeel" not in second.expect("Projects"),
    )
    case.check("nothing is duplicated on the second run", home.state() == before)


run("firstrun", body)
