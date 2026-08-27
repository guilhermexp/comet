"""Aggregate Herdr reporting over the real TUI and hook listener.

Herdr itself is replaced by a local newline-delimited Unix socket server, but
everything on the Unpeel side is real: model derivation, HTTP hooks, debounce,
the reporter worker, and shutdown cleanup.
"""

import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from harness import post_hook, run, tui_hook_port  # noqa: E402


PANE_ID = "w1:p1"
SOURCE = "custom:unpeel"
AGENT = "unpeel"


def report_after(herdr, state, offset):
    """Return the first matching report after an earlier request snapshot."""
    for request in herdr.requests("pane.report_agent")[offset:]:
        if request.get("params", {}).get("state") == state:
            return request
    return None


def body(case):
    home = case.home
    home.project("p", "unpeel", "/tmp")

    title_secret = "private customer renewal session"
    command_secret = "claude --resume provider-conversation-secret"
    cwd_secret = "/private/customer/renewal"
    output_secret = "the renewal total is private"
    hook_secret = "approve the private production operation"
    session_id = "s-private-live"
    home.session(
        session_id,
        label=title_secret,
        command=command_secret,
        project_id="p",
        cwd=cwd_secret,
        output=output_secret + "\r\n",
        running=True,
        settled=True,
    )

    herdr = case.herdr(pane_id=PANE_ID)
    tui = case.pty(
        env={
            "HERDR_ENV": "1",
            "HERDR_SOCKET_PATH": herdr.path,
            "HERDR_PANE_ID": PANE_ID,
        }
    )

    initial = tui.wait_for(
        lambda: report_after(herdr, "idle", 0), timeout=15, poll=0.2
    )
    case.check("initial settled fleet reports idle", initial is not None)
    if initial:
        params = initial.get("params", {})
        case.check(
            "aggregate claims the inherited Herdr pane",
            params.get("pane_id") == PANE_ID
            and params.get("source") == SOURCE
            and params.get("agent") == AGENT,
            json.dumps(params, sort_keys=True),
        )

    port = tui_hook_port(home)
    case.check("TUI hook listener is available", port is not None)
    if port is None:
        return

    report_count = len(herdr.requests("pane.report_agent"))
    case.check(
        "Start hook is accepted",
        post_hook(port, session_id, "Start") == 200,
    )
    working = tui.wait_for(
        lambda: report_after(herdr, "working", report_count),
        timeout=15,
        poll=0.2,
    )
    case.check("Start reports aggregate working", working is not None)

    report_count = len(herdr.requests("pane.report_agent"))
    case.check(
        "PermissionRequest hook is accepted",
        post_hook(
            port,
            session_id,
            "PermissionRequest",
            body={
                "message": hook_secret,
                "prompt": hook_secret,
                "tool_input": {"command": command_secret, "path": cwd_secret},
                "transcript_path": cwd_secret + "/transcript.jsonl",
                "session_id": "provider-session-secret",
            },
        )
        == 200,
    )
    blocked = tui.wait_for(
        lambda: report_after(herdr, "blocked", report_count),
        timeout=15,
        poll=0.2,
    )
    case.check("PermissionRequest reports aggregate blocked", blocked is not None)

    report_count = len(herdr.requests("pane.report_agent"))
    case.check(
        "Stop hook is accepted",
        post_hook(port, session_id, "Stop") == 200,
    )
    idle = tui.wait_for(
        lambda: report_after(herdr, "idle", report_count),
        timeout=15,
        poll=0.2,
    )
    case.check("Stop reports aggregate idle after debounce", idle is not None)

    reports = herdr.requests("pane.report_agent")
    wire = json.dumps(herdr.requests(), sort_keys=True)
    leak_probes = (
        title_secret,
        command_secret,
        cwd_secret,
        output_secret,
        hook_secret,
        session_id,
        "provider-session-secret",
    )
    case.check(
        "Herdr reports contain aggregate counts only",
        reports
        and all(secret not in wire for secret in leak_probes)
        and all(
            "agent_session_id" not in request.get("params", {})
            for request in reports
        ),
        wire,
    )
    allowed = {
        "ping",
        "pane.current",
        "pane.list",
        "pane.report_agent",
        "pane.release_agent",
    }
    case.check(
        "reporter uses only its allowlisted Herdr methods",
        all(request.get("method") in allowed for request in herdr.requests()),
        wire,
    )
    case.check(
        "all Herdr requests are newline-delimited JSON",
        not herdr.parse_errors(),
        str(herdr.parse_errors()),
    )

    release_count = len(herdr.requests("pane.release_agent"))
    tui.send("q", settle=0)
    release = tui.wait_for(
        lambda: herdr.requests("pane.release_agent")[release_count:],
        timeout=8,
        poll=0.1,
    )
    case.check("q releases Herdr lifecycle authority", bool(release))
    if release:
        params = release[-1].get("params", {})
        case.check(
            "release matches the aggregate authority",
            params.get("pane_id") == PANE_ID
            and params.get("source") == SOURCE
            and params.get("agent") == AGENT,
            json.dumps(params, sort_keys=True),
        )

    exited = tui.exited(timeout=8)
    case.check("TUI exits cleanly after release", exited)
    case.check(
        "Herdr reporting does not crash the TUI",
        tui.returncode == 0,
        f"return code: {tui.returncode}",
    )


run("herdr", body)
