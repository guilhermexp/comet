"""Controller scope: ``unpeel --host ssh://…`` is a pure remote client.

The transport uses the real SSH stdio gateway and Host contract; only the
SSH executable itself is replaced by the debug-only test hook so the suite
does not need an sshd.  Controller state starts blank and must stay blank.

Remote scope is the SAME UI as local (host-controller-transports.md): the
same sidebar, modals, and verbs — rename, pin, archive + restore here run
through the shared dialogs against the real gateway.  The only visible
difference is the green Host name on the sidebar's bottom edge.
"""

import json
import os
import shlex
import socket
import struct
import subprocess
import sys

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from harness import CRATES, run  # noqa: E402


class GatewayClient:
    """Minimal UPL1-framed client for `unpeel-host __remote_stdio__` — the
    same wire a real SSH Controller speaks, so requests here exercise the
    production gateway path end to end."""

    def __init__(self, fake_ssh):
        self.proc = subprocess.Popen(
            [fake_ssh], stdin=subprocess.PIPE, stdout=subprocess.PIPE
        )

    def call(self, request_id, method, path, body=None):
        payload = json.dumps(
            {
                "id": request_id,
                "method": method,
                "path": path,
                "query": {},
                "auth": None,
                "contentType": "application/json" if body is not None else None,
                "bodyB64": (
                    __import__("base64").b64encode(json.dumps(body).encode()).decode()
                    if body is not None
                    else None
                ),
            }
        ).encode()
        frame = b"UPL1" + bytes([1, 0, 0, 0]) + struct.pack(">I", len(payload)) + payload
        self.proc.stdin.write(frame)
        self.proc.stdin.flush()
        header = self._read_exact(12)
        assert header[:4] == b"UPL1" and header[4] == 2, repr(header)
        length = struct.unpack(">I", header[8:12])[0]
        envelope = json.loads(self._read_exact(length))
        body_bytes = (
            __import__("base64").b64decode(envelope["bodyB64"])
            if envelope.get("bodyB64")
            else b"{}"
        )
        return envelope["status"], json.loads(body_bytes)

    def _read_exact(self, count):
        data = b""
        while len(data) < count:
            chunk = self.proc.stdout.read(count - len(data))
            if not chunk:
                break
            data += chunk
        return data

    def close(self):
        self.proc.stdin.close()
        self.proc.wait(timeout=10)


def body(case):
    host_home = case.home
    host_home.project("p", "Remote Mac", "/tmp")
    # A second Host project plus a shared project-order.json that CONTRADICTS
    # app-state order: a drag persisted on the Host. The bootstrap must
    # advertise this display order, or every Controller scrambles it.
    host_home.project("q", "Second Mac", "/tmp")
    with open(host_home.path("project-order.json"), "w") as handle:
        json.dump(["q", "p"], handle)
    host_home.session(
        "remote-1",
        label="Host session",
        project_id="p",
        running=True,
        output="output from the Host only\r\n",
    )
    session_host = case.host("remote-1")
    # This simultaneously live row is deliberately the first surviving
    # sidebar fallback while remote-1 is replaced. Restore & Resume must
    # follow the replacement's stable identity instead of selecting it.
    host_home.session(
        "remote-decoy",
        label="unrelated live session",
        command="unsupported-shell",
        project_id="p",
        running=True,
        created_at=1_754_100_000_000,
        output="UNRELATED_REMOTE_TERMINAL\r\n",
    )
    # A pre-filed session proves the archive library is fed by the Host.
    host_home.session(
        "remote-2",
        label="filed session",
        project_id="p",
        running=False,
        created_at=1_754_200_000_000,
    )
    host_home.marker("remote-2", "archived.json", {"archived_at": 1_754_200_000_000})

    fixture_root = host_home.path("remote-controller-fixture")
    controller_home = os.path.join(fixture_root, "home")
    controller_state = os.path.join(fixture_root, "state")
    host_user_home = host_home.path("ssh-user")
    agent_bin = host_home.path("agent-bin")
    os.makedirs(controller_home)
    os.makedirs(host_user_home)
    os.makedirs(agent_bin)
    fake_claude = os.path.join(agent_bin, "claude")
    with open(fake_claude, "w") as handle:
        handle.write(
            "#!/bin/sh\n"
            "printf 'EXACT_REPLACEMENT_SELECTED\\n'\n"
            "sleep 30\n"
        )
    os.chmod(fake_claude, 0o755)

    host_binary = os.path.join(CRATES, "target", "debug", "unpeel-host")
    fake_ssh = host_home.path("fake-ssh")
    script = "\n".join(
        [
            "#!/bin/sh",
            "unset UNPEEL_TEST_SSH_PROGRAM",
            f"export HOME={shlex.quote(host_user_home)}",
            f"export UNPEEL_HOME={shlex.quote(host_home.root)}",
            f"export PATH={shlex.quote(agent_bin)}:$PATH",
            f"exec {shlex.quote(host_binary)} __remote_stdio__",
            "",
        ]
    )
    with open(fake_ssh, "w") as handle:
        handle.write(script)
    os.chmod(fake_ssh, 0o755)

    tui = case.pty(
        args=("--host", "ssh://studio"),
        rows=45,
        cols=150,
        env={
            "HOME": controller_home,
            "UNPEEL_HOME": controller_state,
            "UNPEEL_TEST_SSH_PROGRAM": fake_ssh,
        },
    )

    # The sidebar is the LOCAL UI (" Projects " title); the host indicator on
    # the bottom edge shows the Host's advertised name (its short hostname —
    # the gateway runs on this machine).
    host_name = socket.gethostname()
    if host_name.endswith(".local"):
        host_name = host_name[: -len(".local")]
    indicator = host_name[:12] or "studio"
    screen = tui.expect(
        "Projects", "Host session", "output from the Host only", indicator, timeout=20
    )
    case.check(
        "the local sidebar chrome plus the Host-name indicator own the frame",
        all(
            text in screen
            for text in ("Projects", "Host session", "output from the Host only", indicator)
        ),
        screen[:240],
    )
    case.check(
        "the archived Host session stays out of the sidebar list",
        "filed session" not in tui.sidebar(),
        tui.sidebar()[:240],
    )
    sidebar = tui.sidebar()
    case.check(
        "project order follows the Host's project-order.json, not app-state order",
        0 <= sidebar.find("Second Mac") < sidebar.find("Remote Mac"),
        sidebar[:240],
    )

    fitted = tui.wait_for(lambda: session_host.resizes, timeout=12)
    case.check("the Controller fits the Host PTY", bool(fitted), str(session_host.resizes))
    if session_host.resizes:
        columns, rows = session_host.resizes[-1]
        case.check(
            "the fit uses the preview pane geometry",
            0 < columns < 150 and 0 < rows < 45,
            f"{columns}x{rows} inside a 150x45 Controller",
        )

    grid = tui.grid()
    sidebar_width = grid.sidebar_width()
    session_row = next(
        row
        for row in range(grid.rows)
        if "Host session" in grid.row(row)[:sidebar_width]
    )
    tui.click(5, session_row)
    tui.type("remote input")
    forwarded = tui.wait_for(lambda: "remote input" in session_host.written(), timeout=12)
    case.check(
        "clicking a Host Session focuses it for typing immediately",
        bool(forwarded),
        session_host.written()[:120],
    )

    tui.send(b"\x1d", settle=0.3)  # Ctrl+] detaches from the remote terminal.

    # ── e: the shared rename dialog commits through the Host ──
    tui.send("e", settle=0.5)
    case.check(
        "the shared rename dialog opens in remote scope",
        "rename session" in tui.expect("rename session"),
    )
    tui.type(" on Host")
    tui.send("\r", settle=0.5)
    renamed = tui.wait_for(
        lambda: (host_home.read_marker("remote-1", "title.json") or {}).get("title")
        == "Host session on Host",
        timeout=12,
    )
    case.check(
        "rename lands on the Host as the shared title marker",
        bool(renamed),
        str(host_home.read_marker("remote-1", "title.json")),
    )
    tui.expect("Host session on Host", timeout=12)

    # ── p: pin through the Host, sidebar star follows the bootstrap ──
    tui.send("p", settle=0.5)
    starred = tui.wait_for(lambda: "⭑" in tui.sidebar(), timeout=12)
    case.check("pin lands on the Host and stars the sidebar row", bool(starred), tui.sidebar()[:240])

    # ── s: stop-and-archive, the same verb as local ──
    tui.send("s", settle=0.5)
    archived = tui.wait_for(
        lambda: host_home.has_marker("remote-1", "archived.json"), timeout=15
    )
    case.check(
        "s stops and archives on the Host (archived marker written)",
        bool(archived),
        str(host_home.read_marker("remote-1", "archived.json")),
    )

    # ── a: the shared archive library, fed by the Host's archive list ──
    tui.send("a", settle=0.8)
    screen = tui.expect("Archive", "filed session", timeout=15)
    case.check(
        "the archive library lists the Host's filed sessions",
        "Archive" in screen and "filed session" in screen,
        screen[:240],
    )

    # ── Restore & Resume the highlighted (newest-first) entry ──
    case.check(
        "a resumable archived Host session offers Restore & Resume",
        "Restore & Resume" in screen,
        screen[:240],
    )
    tui.send("\r", settle=0.8)
    restored = tui.wait_for(
        lambda: not host_home.has_marker("remote-1", "archived.json"), timeout=15
    )
    case.check(
        "Restore & Resume clears the Host's archived marker",
        bool(restored),
        str(host_home.read_marker("remote-1", "archived.json")),
    )
    back = tui.wait_for(lambda: "Host session on Host" in tui.sidebar(), timeout=15)
    replacement = None

    def replacement_arrived():
        nonlocal replacement
        replacement = next(
            (
                (session_id, manifest)
                for session_id, manifest in host_home.manifests().items()
                if manifest.get("session", {}).get("label") == "Host session on Host"
                and session_id != "remote-1"
            ),
            None,
        )
        return replacement is not None

    replacement_ready = tui.wait_for(replacement_arrived, timeout=15)
    case.check(
        "the archived session resumes through a replacement Host",
        bool(back) and bool(replacement_ready),
        str((tui.screen()[:480], replacement, host_home.manifests())),
    )
    exact_selection = tui.wait_for(
        lambda: "EXACT_REPLACEMENT_SELECTED" in tui.preview_text(0.3), timeout=15
    )
    preview = tui.preview_text()
    case.check(
        "Restore & Resume selects the exact replacement instead of another live row",
        bool(exact_selection) and "UNRELATED_REMOTE_TERMINAL" not in preview,
        preview[:480],
    )

    # ── ,: the one intentionally missing verb ──
    tui.send(",", settle=0.5)
    case.check(
        "Host settings are declined with one clear line",
        "Host settings aren't editable over this connection yet"
        in tui.expect("Host settings aren't editable"),
    )

    tui.send("q", settle=0.5)
    exited = tui.exited()
    case.check(
        "the remote Controller exits cleanly",
        exited and tui.returncode == 0,
        f"exited={exited} returncode={tui.returncode}",
    )
    case.check(
        "Controller HOME stays empty",
        os.listdir(controller_home) == [],
        repr(os.listdir(controller_home)),
    )
    case.check(
        "Controller UNPEEL_HOME is never created",
        not os.path.exists(controller_state),
        controller_state,
    )

    # ── project.organization.set over the real gateway wire ──
    # Bootstrap advertises the Host's display order; a sortOrder patch (the
    # Controller half of a project drag) lands in the Host's shared
    # project-order.json; the next bootstrap advertises the new order.
    gateway = GatewayClient(fake_ssh)
    try:
        status, bootstrap = gateway.call(1, "GET", "/mobile/bootstrap")
        order = [project["id"] for project in bootstrap.get("projects", [])]
        case.check(
            "gateway bootstrap projects follow project-order.json",
            status == 200 and order == ["q", "p"],
            f"status={status} order={order}",
        )
        ranks = [project.get("sortOrder") for project in bootstrap.get("projects", [])]
        case.check(
            "advertised sortOrder is the display rank (agrees with array order)",
            ranks == [0, 1],
            str(ranks),
        )
        capabilities = (bootstrap.get("hostProtocol") or {}).get("capabilities", [])
        case.check(
            "the gateway advertises project.organization.set",
            "project.organization.set" in capabilities,
            str(capabilities)[:240],
        )

        status, receipt = gateway.call(
            2,
            "POST",
            "/mobile/project-organization",
            body={"projectID": "p", "sortOrder": 0},
        )
        case.check(
            "a sortOrder patch through the gateway succeeds",
            status == 200 and receipt.get("ok") is True,
            f"status={status} body={receipt}",
        )
        with open(host_home.path("project-order.json")) as handle:
            shared = json.load(handle)
        case.check(
            "the patch lands in the Host's shared project-order.json",
            shared == ["p", "q"],
            str(shared),
        )

        status, bootstrap = gateway.call(3, "GET", "/mobile/bootstrap")
        order = [project["id"] for project in bootstrap.get("projects", [])]
        case.check(
            "the next bootstrap advertises the patched order",
            status == 200 and order == ["p", "q"],
            f"status={status} order={order}",
        )

        status, error = gateway.call(
            4,
            "POST",
            "/mobile/project-organization",
            body={"projectID": "p", "folderID": "somewhere"},
        )
        case.check(
            "a legacy folder move is rejected as unimplemented, never ignored",
            status == 501,
            f"status={status} body={error}",
        )
    finally:
        gateway.close()


run("remote_host", body)
