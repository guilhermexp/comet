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


GIB = 1024**3
POLL_SECONDS = 2
# Fixed name so rustc sees the same absolute paths every run: a random target
# path changes sccache's keys and turned every verification into a cold build.
# The flock serializes runs, and the target is still removed at the end.
TARGET_NAME = "target-verify"


def state_dir() -> Path:
    override = os.environ.get("COMET_CARGO_VERIFY_STATE_DIR")
    return Path(override).expanduser() if override else Path.home() / ".local/state/comet/cargo-verify"


def terminate_group(process: subprocess.Popen[bytes], signum: int) -> None:
    if process.poll() is not None:
        return
    try:
        os.killpg(process.pid, signum)
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
            if pgid <= 0:
                raise ValueError("invalid process group")
        except (OSError, ValueError):
            print(f"Refusing unowned verification target: {target}", file=sys.stderr)
            return False
        try:
            os.killpg(pgid, 0)
        except ProcessLookupError:
            shutil.rmtree(target)
            print(f"Removed stale Cargo verification target: {target}", file=sys.stderr)
        except PermissionError:
            print(f"Verification target may still be in use: {target}", file=sys.stderr)
            return False
        else:
            print(f"Verification target is still in use: {target}", file=sys.stderr)
            return False
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

        target = root / TARGET_NAME
        target.mkdir(mode=0o700)
        try:
            env = os.environ.copy()
            env["CARGO_TARGET_DIR"] = str(target)
            env.setdefault("CARGO_BUILD_JOBS", "4")
            # Leave CARGO_INCREMENTAL unset: the dev profile then rebuilds
            # workspace crates incrementally inside a batched run, while sccache
            # (which refuses the variable outright) still caches dependencies.
            # Background QoS pins the build to the efficiency cores on Apple
            # Silicon: slower, but the Mac stays cool and responsive while
            # agents verify. COMET_CARGO_VERIFY_FOREGROUND=1 opts out.
            if sys.platform == "darwin" and not os.environ.get("COMET_CARGO_VERIFY_FOREGROUND"):
                command = ["taskpolicy", "-b", *command]
            print(f"Cargo verification target: {target}", file=sys.stderr, flush=True)
            process = subprocess.Popen(command, env=env, start_new_session=True)
            (Path(target) / ".owner-pgid").write_text(str(process.pid))
            previous_handlers = {}

            def forward(signum: int, _frame: object) -> None:
                terminate_group(process, signum)

            for signum in (signal.SIGINT, signal.SIGTERM):
                previous_handlers[signum] = signal.signal(signum, forward)
            try:
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
                        terminate_group(process, signal.SIGTERM)
                        try:
                            process.wait(timeout=10)
                        except subprocess.TimeoutExpired:
                            terminate_group(process, signal.SIGKILL)
                            process.wait()
                        return 75
            finally:
                for signum, previous in previous_handlers.items():
                    signal.signal(signum, previous)
                if process.poll() is None:
                    terminate_group(process, signal.SIGTERM)
                    try:
                        process.wait(timeout=10)
                    except subprocess.TimeoutExpired:
                        terminate_group(process, signal.SIGKILL)
                        process.wait()
        finally:
            print(f"Removing Cargo verification target: {target}", file=sys.stderr, flush=True)
            shutil.rmtree(target, ignore_errors=True)


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
