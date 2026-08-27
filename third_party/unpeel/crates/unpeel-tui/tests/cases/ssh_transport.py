"""SSH-equivalent transport: `ssh box && unpeel`.

No literal sshd here, so this reproduces exactly what SSH imposes that a
local console doesn't — a conservative TERM with no kitty keyboard
protocol, resize delivered as SIGWINCH (which is precisely how an SSH
client forwards a window change), and a real hosted session attached
through the control socket. If this holds, `ssh <host> && unpeel` holds:
the TUI is an ordinary pty program over a real host.
"""

import sys, os

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from harness import run  # noqa: E402


def body(case):
    home = case.home
    home.project("p", "unpeel", "/tmp")
    home.session("live-1", label="live session", project_id="p", running=True)
    host = case.host("live-1", content="agent output over ssh")

    # A deliberately conservative terminal: plain xterm, no kitty. A real
    # SSH client that never answers the keyboard-enhancement push looks
    # like this — the risk being that startup blocks on the probe or the
    # protocol keys hang. (We push flags unconditionally and never wait.)
    tui = case.pty(env={"TERM": "xterm"})
    tui.read_for(3.0)
    sidebar = tui.sidebar()

    case.check("the TUI renders over a bare terminal", "unpeel" in sidebar, sidebar[:160])
    case.check("the session lists", "live session" in sidebar, sidebar[:160])
    case.check(
        "startup did not stall on the keyboard probe",
        "Projects" in sidebar,
        "a blocked capability query would leave the frame unpainted",
    )

    # Attach and type — the control-socket path, which is what actually
    # matters for driving a session remotely.
    tui.send("\r", settle=1.2)          # focus the session
    tui.type("echo hi")
    forwarded = host.written()
    case.check("keystrokes reach the session over the pty", "echo hi" in forwarded,
               forwarded[:80])
    tui.send(b"\x1d", settle=0.6)        # ctrl+] detach

    # ctrl+1 with no kitty support must degrade, not hang or crash.
    tui.send(b"\x1b[49;5u", settle=0.4)  # kitty ctrl+1 an old client won't send
    tui.send("1", settle=0.4)            # what it sends instead — a plain '1'
    case.check("conservative-terminal keys do not wedge it",
               "unpeel" in tui.sidebar())

    # Resize the window: SSH forwards this as SIGWINCH, exactly what the
    # harness delivers via TIOCSWINSZ. The layout must reflow, not corrupt.
    tui.resize_window(100, 30)
    tui.read_for(1.0)
    case.check("survives a shrink (SIGWINCH)", "live session" in tui.sidebar(),
               tui.sidebar()[:160])
    tui.resize_window(160, 45)
    tui.read_for(1.0)
    g = tui.grid()
    # A real reflow paints the full-width top border across the new width;
    # a corrupt/stale layout would not span it.
    case.check("resize (SIGWINCH) reflows to the new width",
               "live session" in g.sidebar() and g.row(0).rstrip().endswith("\u2510"),
               repr(g.row(0)[:40]) + " … " + repr(g.row(0)[-10:]))

    # Still alive and interactive after all of it.
    case.check("quits cleanly over the pty", (tui.send("q", settle=0.5) or True) and tui.exited())


run("ssh_transport", body)
