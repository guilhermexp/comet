#!/usr/bin/env python3
"""Run local Cargo verification with one disposable target and a disk floor.

The lock covers all Comet checkouts on this Mac. Put a whole verification batch
behind one invocation so its Cargo commands can reuse the temporary target.
"""

from __future__ import annotations

import argparse
import fcntl
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys
import tempfile


GIB = 1024**3
POLL_SECONDS = 2
# Leads the child's process group and kills it once the wrapper's pipe closes,
# including when the wrapper itself is SIGKILLed.
WATCHDOG = "import os, signal, sys; sys.stdin.buffer.read(); os.killpg(0, signal.SIGKILL)"


def state_dir() -> Path:
    override = os.environ.get("COMET_CARGO_VERIFY_STATE_DIR")
    return Path(override).expanduser() if override else Path.home() / ".local/state/comet/cargo-verify"


def terminate_group(pgid: int, signum: int) -> None:
    try:
        os.killpg(pgid, signum)
    except ProcessLookupError:
        pass


def remove_stale_targets(root: Path) -> bool:
    """Reap targets left by a killed wrapper; keep any live child untouched."""
    for target in root.glob("target-*"):
        if target.is_symlink() or not target.is_dir():
            print(f"Refusing unexpected verification target: {target}", file=sys.stderr)
            return False
        try:
            pgid = int((target / ".owner-pgid").read_text())
        except (OSError, ValueError):
            pgid = 0
        if pgid > 0:
            try:
                os.killpg(pgid, 0)
            except ProcessLookupError:
                pass
            except PermissionError:
                print(f"Verification target may still be in use: {target}", file=sys.stderr)
                return False
            else:
                print(f"Verification target is still in use: {target}", file=sys.stderr)
                return False
        shutil.rmtree(target)
        print(f"Removed stale Cargo verification target: {target}", file=sys.stderr)
    return True


def run(command: list[str], minimum_free_gib: int, floor_free_gib: int) -> int:
    root = state_dir()
    root.mkdir(parents=True, exist_ok=True, mode=0o700)
    with (root / "lock").open("a+b") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        if not remove_stale_targets(root):
            return 75
        free = shutil.disk_usage(root).free
        if free < minimum_free_gib * GIB:
            print(
                f"Cargo verification refused: {free / GIB:.1f} GiB free; "
                f"requires {minimum_free_gib} GiB before starting.",
                file=sys.stderr,
            )
            return 75

        with tempfile.TemporaryDirectory(prefix="target-", dir=root) as target:
            env = os.environ.copy()
            env["CARGO_TARGET_DIR"] = target
            env.setdefault("CARGO_BUILD_JOBS", "4")
            env.setdefault("CARGO_INCREMENTAL", "0")
            print(f"Cargo verification target: {target}", file=sys.stderr, flush=True)
            received: list[int] = []
            pgid: int | None = None

            def forward(signum: int, _frame: object) -> None:
                received.append(signum)
                if pgid is not None:
                    terminate_group(pgid, signum)

            previous_handlers = {
                signum: signal.signal(signum, forward)
                for signum in (signal.SIGINT, signal.SIGTERM, signal.SIGHUP)
            }
            read_end, write_end = os.pipe()
            watchdog = None
            try:
                try:
                    watchdog = subprocess.Popen(
                        [sys.executable, "-c", WATCHDOG], stdin=read_end, preexec_fn=os.setpgrp
                    )
                finally:
                    os.close(read_end)
                (Path(target) / ".owner-pgid").write_text(str(watchdog.pid))
                process = subprocess.Popen(
                    command, env=env, preexec_fn=lambda: os.setpgid(0, watchdog.pid)
                )
                pgid = watchdog.pid
                if received:
                    terminate_group(pgid, received[0])
                while True:
                    try:
                        return process.wait(timeout=POLL_SECONDS)
                    except subprocess.TimeoutExpired:
                        free = shutil.disk_usage(root).free
                        if free >= floor_free_gib * GIB:
                            continue
                        print(
                            f"Cargo verification stopped: disk fell to {free / GIB:.1f} GiB "
                            f"free (floor {floor_free_gib} GiB).",
                            file=sys.stderr,
                            flush=True,
                        )
                        terminate_group(pgid, signal.SIGTERM)
                        try:
                            process.wait(timeout=10)
                        except subprocess.TimeoutExpired:
                            terminate_group(pgid, signal.SIGKILL)
                            process.wait()
                        return 75
            finally:
                if pgid is not None and process.poll() is None:
                    terminate_group(pgid, signal.SIGTERM)
                    try:
                        process.wait(timeout=10)
                    except subprocess.TimeoutExpired:
                        terminate_group(pgid, signal.SIGKILL)
                        process.wait()
                os.close(write_end)
                if watchdog is not None:
                    watchdog.wait()
                for signum, previous in previous_handlers.items():
                    signal.signal(signum, previous)
                print(f"Removing Cargo verification target: {target}", file=sys.stderr, flush=True)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--minimum-free-gib", type=int, default=60)
    parser.add_argument("--floor-free-gib", type=int, default=20)
    parser.add_argument("command", nargs=argparse.REMAINDER)
    args = parser.parse_args()
    command = args.command[1:] if args.command[:1] == ["--"] else args.command
    if not command:
        parser.error("give a command after --")
    if args.floor_free_gib < 0 or args.minimum_free_gib <= args.floor_free_gib:
        parser.error("minimum-free-gib must exceed non-negative floor-free-gib")
    try:
        return run(command, args.minimum_free_gib, args.floor_free_gib)
    except BlockingIOError:
        print("Another Comet Cargo verification is running; wait for it to finish.", file=sys.stderr)
        return 75


if __name__ == "__main__":
    raise SystemExit(main())
