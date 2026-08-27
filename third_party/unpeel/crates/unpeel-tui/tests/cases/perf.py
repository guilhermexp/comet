"""Cost guards. A monitor you leave open all day must be nearly free when
nothing is happening — an idle TUI that polls hard or animates hot would
drain a laptop, and the regression is invisible without a test.

Thresholds are deliberately loose: they catch an order-of-magnitude
regression (a poll loop losing its throttle, a shimmer redrawing every
frame), not normal variation between machines."""

import sys, os, time, subprocess

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from harness import run  # noqa: E402


def cpu_seconds(pid):
    """Total CPU time the process has used, via ps."""
    result = subprocess.run(
        ["ps", "-o", "time=", "-p", str(pid)], capture_output=True, text=True
    )
    raw = result.stdout.strip()
    if not raw:
        return None
    parts = [float(p) for p in raw.replace("-", ":").split(":")]
    seconds = 0.0
    for part in parts:
        seconds = seconds * 60 + part
    return seconds


def body(case):
    home = case.home
    home.project("p", "unpeel", "/tmp")
    # A busy-looking fleet: spinners and shimmering headers are the
    # animations most likely to spin the CPU.
    for index in range(6):
        home.session(f"s-{index}", label=f"session {index}", project_id="p",
                     running=True, created_at=1_754_000_000_000 + index)
    hosts = [case.host(f"s-{index}") for index in range(6)]

    tui = case.pty()
    tui.read_for(5.0)

    # ── idle snapshot polling ──
    for host in hosts:
        host.snapshot_requests = 0
    start = time.time()
    tui.read_for(8.0)
    elapsed = time.time() - start
    total = sum(host.snapshot_requests for host in hosts)
    rate = total / elapsed
    case.note(f"idle snapshot rate: {rate:.1f}/s across {len(hosts)} sessions")
    case.check(
        "idle snapshot polling stays throttled",
        rate < 6.0,
        f"{rate:.1f} requests/s — the throttle is what keeps an open TUI cheap",
    )
    selected = max(host.snapshot_requests for host in hosts)
    case.check(
        "only the visible session is polled hard",
        selected >= total - selected,
        f"selected {selected} of {total} — background sessions must stay quiet",
    )

    # ── idle CPU ──
    before = cpu_seconds(tui.pid)
    tui.read_for(8.0)
    after = cpu_seconds(tui.pid)
    if before is None or after is None:
        case.note("ps gave no CPU time — skipped the CPU guard")
    else:
        used = after - before
        case.note(f"idle CPU over 8s: {used:.2f}s")
        case.check(
            "an idle TUI stays close to free",
            used < 2.0,
            f"{used:.2f}s of CPU in 8s idle — spinners/shimmer may be redrawing hot",
        )

    # ── a repaint of a full screen is not pathological ──
    start = time.time()
    tui.frame(0.5)
    case.check(
        "a full repaint is prompt",
        time.time() - start < 4.0,
        f"{time.time() - start:.1f}s for one forced repaint",
    )


run("perf", body)
