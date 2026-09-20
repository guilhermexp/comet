#!/usr/bin/env python3
"""Integration checks for the macOS Cargo runner.

These tests exercise the runner with a tiny executable fixture instead of
building the application.  The fixture is deliberately placed below a
temporary ``target/<profile>`` directory with spaces in its path, matching
the path shape Cargo gives a target runner.
"""

from __future__ import annotations

import json
import os
from pathlib import Path
import plistlib
import select
import signal
import subprocess
import sys
import tempfile
import time
import unittest


RUNNER = Path(__file__).resolve().with_name("cargo-macos-runner")


FIXTURE_TEMPLATE = """#!{python}
import json
import os
import signal
import sys

SOURCE_MARKER = {marker!r}
record = {{
    "argv0": sys.argv[0],
    "args": sys.argv[1:],
    "cwd": os.getcwd(),
    "env_marker": os.environ.get("RUNNER_TEST_ENV"),
    "marker": SOURCE_MARKER,
}}
print(json.dumps(record, sort_keys=True), flush=True)
print("fixture-stderr", file=sys.stderr, flush=True)
if os.environ.get("RUNNER_TEST_WAIT") == "1":
    signal.pause()
sys.exit(int(os.environ.get("RUNNER_TEST_EXIT", "0")))
"""


class CargoMacOSRunnerTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        if sys.platform != "darwin":
            raise unittest.SkipTest("the Cargo app runner is macOS-only")
        if not RUNNER.is_file():
            raise AssertionError(f"runner is missing: {RUNNER}")

    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory(prefix="cargo runner fixture ")
        self.root = Path(self.temp.name)
        self.profile = self.root / "target" / "debug"
        self.profile.mkdir(parents=True)
        self.cwd = self.root / "working directory with spaces"
        self.cwd.mkdir()
        self.fixture = self.profile / "zeron"
        self._write_fixture(self.fixture, "initial")

    def tearDown(self) -> None:
        self.temp.cleanup()

    def _write_fixture(self, path: Path, marker: str) -> None:
        path.write_text(
            FIXTURE_TEMPLATE.format(python=sys.executable, marker=marker),
            encoding="utf-8",
        )
        path.chmod(0o755)

    def _environment(self, **updates: str) -> dict[str, str]:
        environment = os.environ.copy()
        environment.update(updates)
        return environment

    def _run(
        self,
        binary: Path,
        *args: str,
        environment: dict[str, str] | None = None,
    ) -> tuple[subprocess.CompletedProcess[str], dict[str, object]]:
        result = subprocess.run(
            [sys.executable, str(RUNNER), str(binary), *args],
            cwd=self.cwd,
            env=environment or self._environment(),
            text=True,
            capture_output=True,
            check=False,
            timeout=20,
        )
        self.assertTrue(
            result.stdout.strip(),
            msg=f"runner produced no stdout (rc={result.returncode}): {result.stderr}",
        )
        record = json.loads(result.stdout.splitlines()[-1])
        return result, record

    def _assert_bundle(self, record: dict[str, object]) -> Path:
        executable = Path(str(record["argv0"])).resolve()
        expected = (self.profile / "Zeron.app" / "Contents" / "MacOS" / "zeron").resolve()
        self.assertEqual(executable, expected)
        self.assertTrue(executable.is_file())

        plist_path = expected.parent.parent / "Info.plist"
        with plist_path.open("rb") as plist_file:
            plist = plistlib.load(plist_file)
        self.assertEqual(plist["CFBundleIdentifier"], "sh.zeron.app")
        self.assertEqual(plist["CFBundleExecutable"], "zeron")
        self.assertNotEqual(plist["CFBundleShortVersionString"], "__VERSION__")
        self.assertTrue((expected.parent.parent / "Resources" / "zeron.icns").is_file())
        return executable

    def test_no_arguments_launches_native_bundle_and_preserves_context(self) -> None:
        result, record = self._run(
            self.fixture,
            environment=self._environment(RUNNER_TEST_ENV="from-test"),
        )

        self.assertEqual(result.returncode, 0)
        self.assertEqual(record["args"], [])
        self.assertEqual(Path(str(record["cwd"])).resolve(), self.cwd.resolve())
        self.assertEqual(record["env_marker"], "from-test")
        self.assertEqual(record["marker"], "initial")
        self.assertIn("fixture-stderr", result.stderr)
        self._assert_bundle(record)

    def test_zeron_url_is_a_headed_launch_argument(self) -> None:
        result, record = self._run(self.fixture, "zeron://project/abc")

        self.assertEqual(result.returncode, 0)
        self.assertEqual(record["args"], ["zeron://project/abc"])
        self._assert_bundle(record)

    def test_help_and_headless_cli_arguments_passthrough(self) -> None:
        for argument in ("--help", "headless"):
            with self.subTest(argument=argument):
                result, record = self._run(self.fixture, argument)
                self.assertEqual(result.returncode, 0)
                self.assertEqual(Path(str(record["argv0"])).resolve(), self.fixture.resolve())
                self.assertEqual(record["args"], [argument])
                self.assertFalse((self.profile / "Zeron.app").exists())

    def test_non_zeron_binary_passthrough_preserves_exit_status(self) -> None:
        fixture = self.profile / "cargo-test-fixture"
        self._write_fixture(fixture, "named-fixture")

        result, record = self._run(
            fixture,
            environment=self._environment(RUNNER_TEST_EXIT="23"),
        )

        self.assertEqual(result.returncode, 23)
        self.assertEqual(Path(str(record["argv0"])).resolve(), fixture.resolve())
        self.assertEqual(record["marker"], "named-fixture")
        self.assertFalse((self.profile / "Zeron.app").exists())

    def test_sigterm_reaches_execed_fixture(self) -> None:
        process = subprocess.Popen(
            [sys.executable, str(RUNNER), str(self.fixture)],
            cwd=self.cwd,
            env=self._environment(RUNNER_TEST_WAIT="1"),
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )
        try:
            assert process.stdout is not None
            ready, _, _ = select.select([process.stdout], [], [], 15)
            self.assertTrue(ready, "fixture did not report readiness before timeout")
            self.assertTrue(process.stdout.readline().strip())

            process.send_signal(signal.SIGTERM)
            returncode = process.wait(timeout=10)
            self.assertEqual(returncode, -signal.SIGTERM)
        finally:
            if process.poll() is None:
                process.terminate()
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait(timeout=5)
            if process.stderr is not None:
                process.stderr.close()
            if process.stdout is not None:
                process.stdout.close()

    def test_rerun_replaces_bundle_with_new_target_binary(self) -> None:
        first_result, first_record = self._run(self.fixture)
        self.assertEqual(first_result.returncode, 0)
        self.assertEqual(first_record["marker"], "initial")
        self._assert_bundle(first_record)

        self._write_fixture(self.fixture, "updated")
        future = time.time() + 2
        os.utime(self.fixture, (future, future))

        second_result, second_record = self._run(self.fixture)
        self.assertEqual(second_result.returncode, 0)
        self.assertEqual(second_record["marker"], "updated")
        self._assert_bundle(second_record)


if __name__ == "__main__":
    unittest.main(verbosity=2)
