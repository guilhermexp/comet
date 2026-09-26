#!/usr/bin/env python3
"""Fast behavior checks for the disposable local Cargo verification target."""

from __future__ import annotations

import os
import signal
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import unittest


WRAPPER = Path(__file__).with_name("cargo-verify.py")


class CargoVerifyTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.state = Path(self.temporary.name) / "state"
        self.env = os.environ.copy()
        self.env["COMET_CARGO_VERIFY_STATE_DIR"] = str(self.state)

    def command(self, code: str) -> list[str]:
        return [
            sys.executable,
            str(WRAPPER),
            "--minimum-free-gib",
            "1",
            "--floor-free-gib",
            "0",
            "--",
            sys.executable,
            "-c",
            code,
        ]

    def targets(self) -> list[Path]:
        return list(self.state.glob("target-*"))

    def test_success_and_failure_remove_their_target(self) -> None:
        marker = Path(self.temporary.name) / "marker"
        code = f"import os, pathlib; pathlib.Path({str(marker)!r}).write_text(os.environ['CARGO_TARGET_DIR'])"
        result = subprocess.run(self.command(code), env=self.env, capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertFalse(Path(marker.read_text()).exists())
        self.assertEqual(self.targets(), [])

        result = subprocess.run(
            self.command("import sys; sys.exit(7)"), env=self.env, capture_output=True, text=True
        )
        self.assertEqual(result.returncode, 7, result.stderr)
        self.assertEqual(self.targets(), [])

    def test_disk_preflight_refuses_before_spawning(self) -> None:
        marker = Path(self.temporary.name) / "should-not-exist"
        command = self.command(f"import pathlib; pathlib.Path({str(marker)!r}).touch()")
        command[command.index("--minimum-free-gib") + 1] = "1000000"
        result = subprocess.run(command, env=self.env, capture_output=True, text=True)
        self.assertEqual(result.returncode, 75)
        self.assertFalse(marker.exists())
        self.assertEqual(self.targets(), [])

    def test_next_run_removes_a_target_left_by_a_dead_wrapper(self) -> None:
        stale = self.state / "target-stale"
        stale.mkdir(parents=True)
        (stale / ".owner-pgid").write_text("99999999")
        (stale / "large-artifact").write_text("regenerable")
        result = subprocess.run(
            self.command("pass"), env=self.env, capture_output=True, text=True
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("Removed stale Cargo verification target", result.stderr)
        self.assertEqual(self.targets(), [])

    def test_second_verification_cannot_allocate_another_target(self) -> None:
        ready = Path(self.temporary.name) / "ready"
        first = subprocess.Popen(
            self.command(
                f"import pathlib, time; pathlib.Path({str(ready)!r}).touch(); time.sleep(1)"
            ),
            env=self.env,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )
        try:
            deadline = time.monotonic() + 5
            while not ready.exists() and time.monotonic() < deadline:
                time.sleep(0.02)
            self.assertTrue(ready.exists(), "first verification did not start")
            second = subprocess.run(
                self.command("pass"), env=self.env, capture_output=True, text=True
            )
            self.assertEqual(second.returncode, 75)
            self.assertIn("Another Comet Cargo verification", second.stderr)
            self.assertEqual(len(self.targets()), 1)
        finally:
            first.communicate(timeout=5)
        self.assertEqual(self.targets(), [])

    def test_termination_stops_child_and_removes_target(self) -> None:
        self.assert_signal_stops_child(signal.SIGTERM)

    def test_next_run_removes_a_target_without_an_owner(self) -> None:
        unowned = self.state / "target-unowned"
        unowned.mkdir(parents=True)
        result = subprocess.run(self.command("pass"), env=self.env, capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.targets(), [])

    def test_hangup_stops_child_and_removes_target(self) -> None:
        self.assert_signal_stops_child(signal.SIGHUP)

    def test_killed_wrapper_takes_its_child_down(self) -> None:
        self.assert_signal_stops_child(signal.SIGKILL, clean_target=False)
        result = subprocess.run(self.command("pass"), env=self.env, capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("Removed stale Cargo verification target", result.stderr)
        self.assertEqual(self.targets(), [])

    def assert_signal_stops_child(self, signum: int, clean_target: bool = True) -> None:
        ready = Path(self.temporary.name) / "ready"
        process = subprocess.Popen(
            self.command(
                f"import os, pathlib, time; pathlib.Path({str(ready)!r}).write_text(str(os.getpid())); time.sleep(30)"
            ),
            env=self.env,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )
        try:
            deadline = time.monotonic() + 5
            while not (ready.exists() and ready.read_text()) and time.monotonic() < deadline:
                time.sleep(0.02)
            child = int(ready.read_text())
            process.send_signal(signum)
            process.communicate(timeout=5)
            deadline = time.monotonic() + 5
            while time.monotonic() < deadline:
                try:
                    os.kill(child, 0)
                except ProcessLookupError:
                    break
                time.sleep(0.02)
            else:
                self.fail("verification child outlived its wrapper")
            if clean_target:
                self.assertEqual(self.targets(), [])
        finally:
            if process.poll() is None:
                process.kill()
                process.communicate(timeout=5)


if __name__ == "__main__":
    unittest.main()
