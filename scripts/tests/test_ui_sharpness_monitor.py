"""CLI and evidence-safety tests for the native sharpness monitor."""

import json
import importlib.util
import os
from pathlib import Path
import subprocess
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

from PIL import Image, ImageDraw


SCRIPT = Path(__file__).resolve().parents[1] / "ui-sharpness-monitor.py"


def monitor_module():
    spec = importlib.util.spec_from_file_location("sharpness_monitor_under_test", SCRIPT)
    if spec is None or spec.loader is None:
        raise RuntimeError("could not load monitor module")
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


class SharpnessMonitorCliTests(unittest.TestCase):
    def invoke(self, *args):
        return subprocess.run(
            [sys.executable, str(SCRIPT), *map(str, args)],
            capture_output=True,
            text=True,
            timeout=20,
        )

    @staticmethod
    def make_frame(path, size=(96, 64), offset=0):
        image = Image.new("RGB", size, (24, 24, 24))
        draw = ImageDraw.Draw(image)
        draw.rectangle((12 + offset, 12, 35 + offset, 44), fill=(230, 230, 230))
        draw.line((48 + offset, 8, 48 + offset, 54), fill=(170, 170, 170), width=2)
        image.save(path, format="PNG")

    def test_offline_replay_records_each_sample_and_private_bounded_artifacts(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            source = root / "frames"
            source.mkdir()
            for index in range(3):
                self.make_frame(source / f"{index:03}.png")
            output = root / "report"

            result = self.invoke("--analyze", source, "--out", output, "--max-mib", "1")

            self.assertEqual(result.returncode, 0, result.stderr)
            report = json.loads((output / "report.json").read_text())
            self.assertEqual(report["samples"], 3)
            self.assertIn("termination_reason", report)
            self.assertTrue(report["limitations"])
            observations = [json.loads(line) for line in (output / "observations.jsonl").read_text().splitlines()]
            self.assertEqual(len(observations), 3)
            self.assertGreaterEqual(len(list(output.rglob("*.png"))), 2)
            self.assertEqual(os.stat(output).st_mode & 0o777, 0o700)
            for path in output.rglob("*"):
                if path.is_file():
                    self.assertEqual(os.stat(path).st_mode & 0o777, 0o600, path)

    def test_jpeg_with_png_suffix_is_rejected_and_partial_report_is_preserved(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            source = root / "frames"
            source.mkdir()
            Image.new("RGB", (48, 32), "white").save(source / "fake.png", format="JPEG")
            output = root / "report"

            result = self.invoke("--analyze", source, "--out", output)

            self.assertNotEqual(result.returncode, 0)
            report = json.loads((output / "report.json").read_text())
            self.assertEqual(report["samples"], 0)
            self.assertEqual(report["termination_reason"], "error")
            self.assertIn("error", report)

    def test_roi_outside_frame_fails_without_losing_partial_report(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            source = root / "frames"
            source.mkdir()
            self.make_frame(source / "000.png", size=(96, 64))
            output = root / "report"

            result = self.invoke(
                "--analyze", source, "--out", output, "--roi", "90,0,20,20"
            )

            self.assertNotEqual(result.returncode, 0)
            report = json.loads((output / "report.json").read_text())
            self.assertEqual(report["termination_reason"], "error")
            self.assertEqual(report["samples"], 0)

    def test_geometry_change_is_recorded_and_rebaselined(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            source = root / "frames"
            source.mkdir()
            self.make_frame(source / "000.png", size=(96, 64))
            self.make_frame(source / "001.png", size=(112, 64))
            self.make_frame(source / "002.png", size=(112, 64))
            output = root / "report"

            result = self.invoke("--analyze", source, "--out", output)

            self.assertEqual(result.returncode, 0, result.stderr)
            events = [json.loads(line) for line in (output / "events.jsonl").read_text().splitlines()]
            self.assertTrue(any(event.get("kind") == "rebaseline" for event in events), events)
            self.assertEqual(json.loads((output / "report.json").read_text())["samples"], 3)
            observations = [json.loads(line) for line in (output / "observations.jsonl").read_text().splitlines()]
            self.assertEqual(observations[1]["region_coverage"][0]["box_pixels"][2], 112)

    def test_streaming_roi_rebaseline_does_not_hide_blur_in_a_static_roi(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            source = root / "frames"
            source.mkdir()
            left = self._glyph_panel(variant=0)
            right = self._glyph_panel(variant=0)
            first = Image.new("RGB", (512, 128), (24, 24, 24))
            first.paste(left, (0, 0))
            first.paste(right, (256, 0))
            second = Image.new("RGB", first.size, (24, 24, 24))
            second.paste(self._glyph_panel(variant=1), (256, 0))
            # Blur only the left crop, leaving the streaming/right crop sharp.
            from PIL import ImageFilter
            second.paste(left.filter(ImageFilter.GaussianBlur(1.2)), (0, 0))
            first.save(source / "000.png")
            second.save(source / "001.png")
            second.save(source / "002.png")
            output = root / "report"

            result = self.invoke(
                "--analyze", source, "--out", output,
                "--roi", "0,0,256,128", "--roi", "256,0,256,128",
            )

            self.assertEqual(result.returncode, 0, result.stderr)
            events = [json.loads(line) for line in (output / "events.jsonl").read_text().splitlines()]
            candidate = next(event for event in events if event["kind"] == "blur_candidate")
            self.assertIn("roi-00", candidate["regions"])
            rebaseline = next(event for event in events if event["kind"] == "rebaseline")
            self.assertIn("roi-01", rebaseline["regions"])

    def test_blur_evidence_uses_the_region_baseline_after_content_rebaseline(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            source = root / "frames"
            source.mkdir()
            first = self._glyph_panel(variant=0)
            replaced = self._glyph_panel(variant=1)
            first.save(source / "000.png")
            replaced.save(source / "001.png")
            from PIL import ImageFilter
            replaced.filter(ImageFilter.GaussianBlur(1.2)).save(source / "002.png")
            output = root / "report"

            result = self.invoke(
                "--analyze", source, "--out", output,
                "--roi", "0,0,256,128",
            )

            self.assertEqual(result.returncode, 0, result.stderr)
            events = [json.loads(line) for line in (output / "events.jsonl").read_text().splitlines()]
            candidate = next(event for event in events if event["kind"] == "blur_candidate")
            self.assertEqual(candidate["evidence_scope"], "region")
            self.assertEqual(candidate["box_pixels"], [0, 0, 256, 128])
            self.assertEqual(candidate["baseline_version"], 1)
            self.assertEqual(candidate["region"], "roi-00")
            before = Image.open(output / candidate["before_png"]).convert("RGBA")
            expected = Image.open(source / "001.png").convert("RGBA")
            after = Image.open(output / candidate["after_png"]).convert("RGBA")
            expected_after = Image.open(source / "002.png").convert("RGBA")
            from PIL import ImageChops
            self.assertIsNone(ImageChops.difference(before, expected).getbbox())
            self.assertIsNone(ImageChops.difference(after, expected_after).getbbox())
            self.assertTrue((output / candidate["context_before_png"]).is_file())
            self.assertTrue((output / candidate["context_after_png"]).is_file())

    def test_enter_pending_during_slow_capture_is_recorded_even_without_sleep(self):
        module = monitor_module()
        with tempfile.TemporaryDirectory() as tmp:
            output = module.OutputStore(Path(tmp) / "report", 1024 * 1024)
            output.initialize()
            output.mode = "live"
            output.configuration = {"duration_seconds": 0.35, "interval_seconds": 0.1}
            monitor = module.Monitor(output, [])
            target = module.WindowTarget(11, 101, "Zeron", "/tmp/zeron", (0, 0, 800, 600))

            class Clock:
                now = 0.0
                enter_pending = False

                def monotonic(self):
                    return self.now

                def sleep(self, seconds):
                    self.now += seconds

                def monotonic_ns(self):
                    return round(self.now * 1_000_000_000)

            clock = Clock()

            class SlowProvider:
                captures = 0

                def preflight(self):
                    return None

                def discover(self, _window_id=None):
                    return target

                def validate(self, _target):
                    return target

                def capture(self, _target):
                    self.captures += 1
                    clock.now += 0.12  # Longer than the interval: next cycle has no sleep to poll in.
                    clock.enter_pending = self.captures == 1
                    return Image.new("RGB", (800, 600), (24, 24, 24)), target

            def poll_enter():
                if clock.enter_pending:
                    clock.enter_pending = False
                    return True
                return False

            args = SimpleNamespace(window_id=None, duration=0.35, interval=0.1, roi=[])
            try:
                from contextlib import redirect_stdout
                import io as io_module
                printed = io_module.StringIO()
                with patch.object(module, "time", SimpleNamespace(
                    monotonic=clock.monotonic, monotonic_ns=clock.monotonic_ns, sleep=clock.sleep,
                )), patch.object(module, "_poll_enter", side_effect=poll_enter), \
                        patch.object(module, "_classify", return_value={"kind": "unchanged"}), \
                        patch.object(module, "_free_disk_check"), redirect_stdout(printed):
                    module._run_live(args, monitor, SlowProvider())
                self.assertIn("Sampling 3 edge-bearing 256px regions", printed.getvalue())
                events = [json.loads(line) for line in (output.path / "events.jsonl").read_text().splitlines()]
                marks = [event for event in events if event["kind"] == "manual_mark"]
                self.assertTrue(marks, events)
                self.assertEqual(marks[0]["sample_index"], 0)
                self.assertEqual(marks[0]["source"], "keyboard")
            finally:
                output.finalize()

    def test_default_edge_tiles_cover_right_side_when_center_is_densest(self):
        module = monitor_module()
        image = Image.new("RGB", (2048, 1024), (24, 24, 24))
        draw = ImageDraw.Draw(image)
        # A dense central Chat transcript should not consume every tile slot.
        for x in range(512, 1536, 5):
            draw.line((x, 0, x, 1023), fill=(230, 230, 230), width=2)
        for y in range(0, 1024, 5):
            draw.line((512, y, 1535, y), fill=(190, 190, 190), width=2)
        # The right-side Workers/Details area is sparse but still legible.
        draw.text((1820, 420), "WORKERS DETAILS", fill=(230, 230, 230), stroke_width=1)

        regions = module._edge_tiles(image)

        self.assertGreater(len(regions), 0)
        self.assertLessEqual(len(regions), module.MAX_TILES)
        self.assertTrue(any(box[0] >= 1792 for _, box in regions), regions)
        self.assertTrue(all("x" in name and "y" in name for name, _ in regions))
        with tempfile.TemporaryDirectory() as tmp:
            output = module.OutputStore(Path(tmp) / "report", 4 * 1024 * 1024)
            output.initialize()
            try:
                monitor = module.Monitor(output, [])
                monitor.observe(image, {"source": "test"})
                observation = json.loads((output.path / "observations.jsonl").read_text().splitlines()[0])
                self.assertEqual(
                    observation["region_coverage"],
                    [
                        {"name": name, "box_pixels": [box[0], box[1], box[2] - box[0], box[3] - box[1]]}
                        for name, box in regions
                    ],
                )
                self.assertEqual(output.configuration["region_coverage"], observation["region_coverage"])
            finally:
                output.finalize()

    @staticmethod
    def _glyph_panel(variant=0):
        image = Image.new("RGB", (256, 128), (24, 24, 24))
        draw = ImageDraw.Draw(image)
        for x in range(12, 240, 24):
            for y in (12, 48, 84):
                shift = 0 if variant == 0 else (5 if (x + y) % 2 else -5)
                draw.rectangle((x + shift, y, x + shift + 3, y + 22), fill=(224, 224, 224))
                draw.rectangle((x + shift, y + 19, x + shift + 15, y + 22), fill=(224, 224, 224))
        return image

    def test_comparison_records_exact_pixel_equality_separately_from_kind(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            source = root / "frames"
            source.mkdir()
            frame = self._glyph_panel()
            frame.save(source / "000.png")
            Image.eval(frame, lambda value: round(value * 0.7 + 12)).save(source / "001.png")
            output = root / "report"

            result = self.invoke("--analyze", source, "--out", output)

            self.assertEqual(result.returncode, 0, result.stderr)
            observations = [json.loads(line) for line in (output / "observations.jsonl").read_text().splitlines()]
            comparison = observations[1]["comparisons"][0]
            self.assertFalse(comparison["pixel_equal"])
            self.assertNotEqual(comparison["reference_sha256"], comparison["current_sha256"])
            self.assertEqual(comparison["kind"], "contrast")

    def test_nonfinite_or_nonpositive_timing_and_budget_are_rejected(self):
        for option, value in (("--duration", "nan"), ("--interval", "0"), ("--max-mib", "inf")):
            with self.subTest(option=option), tempfile.TemporaryDirectory() as tmp:
                root = Path(tmp)
                source = root / "frames"
                source.mkdir()
                output = root / "report"
                result = self.invoke("--analyze", source, "--out", output, option, value)
                self.assertNotEqual(result.returncode, 0)

    def test_nonempty_output_is_never_overwritten(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            source = root / "frames"
            source.mkdir()
            self.make_frame(source / "000.png")
            output = root / "report"
            output.mkdir()
            sentinel = output / "report.json"
            sentinel.write_text('{"preserve": true}\n')

            result = self.invoke("--analyze", source, "--out", output)

            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(sentinel.read_text(), '{"preserve": true}\n')
            self.assertEqual(list(output.iterdir()), [sentinel])

    def test_near_full_capture_budget_still_finalizes_report_within_budget(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            source = root / "frames"
            source.mkdir()
            frame = Image.new("RGB", (24, 24), (80, 80, 80))
            for index in range(100):
                frame.save(source / f"{index:03}.png")
            output = root / "report"

            result = self.invoke(
                "--analyze", source, "--out", output,
                "--duration", "100", "--interval", "1", "--max-mib", "0.03125",
            )

            self.assertEqual(result.returncode, 0, result.stderr)
            report = json.loads((output / "report.json").read_text())
            self.assertEqual(report["termination_reason"], "storage_limit")
            self.assertGreater(report["samples"], 1)
            self.assertLessEqual(sum(path.stat().st_size for path in output.rglob("*") if path.is_file()), 32 * 1024)

    def test_window_discovery_requires_explicit_id_when_zeron_has_multiple_windows(self):
        module = monitor_module()
        provider = object.__new__(module.NativeCaptureProvider)
        targets = [
            module.WindowTarget(11, 101, "Zeron", "/tmp/zeron", (0, 0, 800, 600)),
            module.WindowTarget(12, 101, "Zeron", "/tmp/zeron", (0, 0, 1200, 800)),
        ]
        with patch.object(provider, "_windows", return_value=targets):
            with self.assertRaisesRegex(module.MonitorError, "--window-id"):
                provider.discover()
            self.assertEqual(provider.discover(12).window_id, 12)

    def test_window_identity_rejects_pid_changes_and_comet_name_collision(self):
        module = monitor_module()
        provider = object.__new__(module.NativeCaptureProvider)
        expected = module.WindowTarget(11, 101, "Zeron", "/tmp/zeron", (0, 0, 800, 600))
        with patch.object(
            provider,
            "_windows",
            return_value=[module.WindowTarget(11, 202, "Zeron", "/tmp/zeron", (0, 0, 800, 600))],
        ):
            with self.assertRaisesRegex(module.MonitorError, "PID changed"):
                provider.validate(expected)
        self.assertTrue(module.NativeCaptureProvider._is_zeron("Zeron", "/tmp/zeron"))
        self.assertTrue(module.NativeCaptureProvider._is_zeron("Zeron Dev", "/tmp/zeron"))
        self.assertFalse(module.NativeCaptureProvider._is_zeron("Comet", "/Applications/Comet.app/Contents/MacOS/comet"))
        self.assertFalse(module.NativeCaptureProvider._is_zeron("Zeron", "/tmp/comet"))

    def test_missing_or_wrong_owner_live_target_preserves_partial_report(self):
        module = monitor_module()
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            output = module.OutputStore(root / "report", 1024 * 1024)
            output.initialize()
            output.mode = "live"
            output.configuration = {"duration_seconds": 1.0, "interval_seconds": 1.0}
            monitor = module.Monitor(output, [])

            class WrongOwnerProvider:
                def preflight(self):
                    return None

                def discover(self, _window_id=None):
                    return module.WindowTarget(11, 101, "Zeron", "/tmp/zeron", (0, 0, 800, 600))

                def validate(self, _target):
                    raise module.MonitorError("selected window owner or PID changed")

            try:
                module._run_live(
                    SimpleNamespace(window_id=None, duration=1.0, interval=1.0),
                    monitor,
                    WrongOwnerProvider(),
                )
            except module.MonitorError as exc:
                output.termination_reason = "error"
                output.error = str(exc)
            output.finalize()

            report = json.loads((output.path / "report.json").read_text())
            self.assertEqual(report["termination_reason"], "error")
            self.assertEqual(report["samples"], 0)
            self.assertEqual(report["window"]["pid"], 101)

    def test_live_capture_excludes_window_shadow_and_accepts_png_only(self):
        module = monitor_module()
        provider = object.__new__(module.NativeCaptureProvider)
        target = module.WindowTarget(11, 101, "Zeron", "/tmp/zeron", (0, 0, 800, 600))
        provider.validate = lambda _expected: target
        calls = []

        def fake_screencapture(argv, **kwargs):
            calls.append((argv, kwargs))
            Image.new("RGB", (800, 600), (24, 24, 24)).save(argv[-1], format="PNG")
            return SimpleNamespace(returncode=0, stderr="")

        module_dir = monitor_module()
        with patch.object(module_dir.subprocess, "run", side_effect=fake_screencapture):
            image, seen_target = provider.capture(target)
        self.assertEqual(seen_target.window_id, 11)
        self.assertEqual(image.size, (800, 600))
        self.assertIn("-o", calls[0][0])
        self.assertIn("-l", calls[0][0])

    def test_html_escapes_untrusted_input_filename(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            source = root / "frames"
            source.mkdir()
            self.make_frame(source / '<script>-alert-x.png')
            output = root / "report"

            result = self.invoke("--analyze", source, "--out", output)

            self.assertEqual(result.returncode, 0, result.stderr)
            html = (output / "report.html").read_text()
            self.assertNotIn("<script>", html)
            self.assertIn("&lt;script&gt;", html)

    def test_offline_replay_rejects_manual_mark_outside_sample_range(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            source = root / "frames"
            source.mkdir()
            self.make_frame(source / "000.png")
            output = root / "report"

            result = self.invoke("--analyze", source, "--out", output, "--mark-at", "1")

            self.assertNotEqual(result.returncode, 0)
            report = json.loads((output / "report.json").read_text())
            self.assertEqual(report["termination_reason"], "error")
            self.assertEqual(report["samples"], 1)


if __name__ == "__main__":
    unittest.main()
