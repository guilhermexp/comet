"""MCP write approvals with no desktop app: the TUI must answer the prompt
itself, from its own keyboard or from a paired phone, and persist the grant
where the app would look for it."""

import sys, os, threading, time

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from harness import run, mcp_post, mobile_request, tui_hook_port  # noqa: E402


def body(case):
    home = case.home
    home.project("p", "unpeel", "/tmp")
    home.session("s1", label="a session", project_id="p")
    token = home.pair_device()
    phone_port = home.reserve_mobile_port()
    state = home.state()
    state["mcp_write_approvals"] = {}
    home.write_state(state)

    tui = case.pty()
    tui.read_for(6.0)  # the mobile server needs its app-offline detection cycle

    port = tui_hook_port(home)
    case.check("the TUI serves the bridge itself", port is not None)

    status, _ = mcp_post(port, "/mcp/approve-write",
                         {"caller_session_id": "a", "target_session_id": "b"})
    case.check("an unauthenticated call is rejected", status == 401, str(status))

    # ── approve from the terminal ──
    results = {}

    def ask(name, caller, target):
        results[name] = mcp_post(
            port,
            "/mcp/approve-write",
            {"caller_session_id": caller, "target_session_id": target},
            token=home.auth_token,
        )

    thread = threading.Thread(target=ask, args=("grant", "caller-1", "target-1"))
    thread.start()
    time.sleep(1.5)
    prompt = tui.expect("Allow session")
    case.check("the prompt is shown", "Allow session" in prompt and "y/n" in prompt, prompt[:200])

    tui.send("y", settle=0.5)
    thread.join(timeout=15)
    case.check("y approves", results.get("grant", (0, {}))[1] == {"approved": True},
               str(results.get("grant")))
    persisted = home.state().get("mcp_write_approvals", {})
    case.check("the grant is persisted where the app looks",
               persisted.get("caller-1") == ["target-1"], str(persisted))

    # ── a repeat is immediate, no prompt ──
    ask("fast", "caller-1", "target-1")
    case.check("an approved pair is remembered",
               results["fast"][1] == {"approved": True}, str(results["fast"]))

    # ── deny ──
    thread = threading.Thread(target=ask, args=("deny", "caller-2", "target-2"))
    thread.start()
    time.sleep(1.5)
    tui.send("n", settle=0.5)
    thread.join(timeout=15)
    case.check("n denies", results.get("deny", (0, {}))[1] == {"approved": False},
               str(results.get("deny")))
    case.check("a denial is not persisted",
               "caller-2" not in home.state().get("mcp_write_approvals", {}))

    # ── answer from the phone instead ──
    ready, _ = mobile_request(phone_port, "/mobile/bootstrap", token)
    case.check("the phone server is up", ready == 200, str(ready))
    if ready == 200:
        thread = threading.Thread(target=ask, args=("phone", "caller-3", "target-3"))
        thread.start()
        time.sleep(1.5)
        _, boot = mobile_request(phone_port, "/mobile/bootstrap", token)
        pending = boot.get("pendingApprovals", [])
        case.check(
            "the request reaches the phone",
            len(pending) == 1
            and pending[0]["kind"] == "write"
            and pending[0]["callerSessionID"] == "caller-3",
            str(pending),
        )
        if pending:
            approval_id = pending[0]["id"]
            answer_status, _ = mobile_request(
                phone_port,
                "/mobile/approvals/answer",
                token,
                method="POST",
                body={"id": approval_id, "approved": True},
            )
            repeated_status, repeated_body = mobile_request(
                phone_port,
                "/mobile/approvals/answer",
                token,
                method="POST",
                body={"id": approval_id, "approved": True},
            )
            case.check("the first phone answer is accepted", answer_status == 200,
                       str(answer_status))
            case.check(
                "an already-answered approval conflicts",
                repeated_status == 409
                and repeated_body.get("error") == "approval no longer pending",
                str((repeated_status, repeated_body)),
            )
        thread.join(timeout=15)
        case.check("the phone's answer approves",
                   results.get("phone", (0, {}))[1] == {"approved": True},
                   str(results.get("phone")))


run("approvals", body)
