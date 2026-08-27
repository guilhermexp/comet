"""Provider session ids are shared state, not app state.

Hook payloads carry the provider's own conversation id (Claude's session
UUID, Codex thread ids…). The app used to capture them into ITS
UserDefaults; app-less they were dropped, so a TUI restart lost the exact
conversation and fell back to continue-last. Now whichever frontend
receives the hook broadcast writes the shared `provider-session.json`
marker, and stopped-session Resume on both sides reads marker → manifest → fallback."""

import sys, os, json, time

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from harness import run, post_hook, tui_hook_port  # noqa: E402


def body(case):
    home = case.home
    home.project("p", "unpeel", "/tmp")
    # cursor-agent, deliberately: claude's resume arm pre-flights the id
    # against the REAL ~/.claude conversations (a fabricated id relaunches
    # fresh — correct, and exactly why the test must not use it).
    home.session("s-claude", label="cursor convo", project_id="p",
                 command="cursor-agent")

    tui = case.pty()
    tui.read_for(3.0)
    port = tui_hook_port(home)
    case.check("listener up", port is not None)

    # ── a hook event carrying the provider's session id ──
    status = post_hook(port, "s-claude", "Stop",
                       body={"session_id": "prov-1234-abcd"})
    case.check("hook accepted", status == 200, str(status))
    marker = tui.wait_for(
        lambda: home.read_marker("s-claude", "provider-session.json"), timeout=8
    )
    case.check("the marker is written app-lessly", bool(marker), str(marker))
    if marker:
        case.check(
            "it records the provider id",
            marker.get("provider_session_id") == "prov-1234-abcd",
            str(marker),
        )

    # ── a later transcript-only event merges, never erases ──
    post_hook(port, "s-claude", "Stop",
              body={"transcript_path": "/tmp/prov.jsonl"})
    time.sleep(0.8)
    merged = home.read_marker("s-claude", "provider-session.json")
    case.check(
        "a transcript-only event keeps the id",
        merged.get("provider_session_id") == "prov-1234-abcd"
        and merged.get("provider_transcript_path") == "/tmp/prov.jsonl",
        str(merged),
    )

    # ── stopped-session Resume continues THAT conversation through the marker ──
    tui.send("r", settle=1.0)
    new_manifest = tui.wait_for(
        lambda: next(
            (m for sid, m in home.manifests().items() if sid != "s-claude"),
            None,
        ),
        timeout=25,
    )
    case.check("Resume spawned a replacement", bool(new_manifest))
    if new_manifest:
        command = new_manifest["session"]["command"]
        case.check(
            "the relaunch resumes the captured conversation",
            "prov-1234-abcd" in command,
            command,
        )

    # ── an unknown session must not grow a marker (or a dir) ──
    post_hook(port, "ghost-session", "Stop", body={"session_id": "nope"})
    time.sleep(0.5)
    case.check(
        "no marker for unknown sessions",
        not os.path.exists(home.path("app-sessions", "ghost-session")),
    )


run("provider_capture", body)
