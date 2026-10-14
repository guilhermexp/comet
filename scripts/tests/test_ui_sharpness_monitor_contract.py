"""CLI contract: real PNG evidence must detect known spatial blur offline."""
from pathlib import Path
import json
import subprocess
import sys
import tempfile
import unittest

from PIL import Image, ImageDraw, ImageFilter

SCRIPT = Path(__file__).resolve().parents[1] / "ui-sharpness-monitor.py"


class MonitorContractTests(unittest.TestCase):
    def test_offline_spatial_blur_retains_incident_and_manual_mark(self):
        # A detector reduced to raw edge energy, or a CLI dropping evidence,
        # must fail this end-to-end test.
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            source = root / "source"
            source.mkdir()
            sharp = Image.new("RGB", (256, 128), (24, 24, 24))
            draw = ImageDraw.Draw(sharp)
            for x in range(12, 240, 24):
                for y in (12, 48, 84):
                    draw.rectangle((x, y, x + 3, y + 22), fill=(224, 224, 224))
                    draw.rectangle((x, y + 19, x + 15, y + 22), fill=(224, 224, 224))
            sharp.save(source / "000.png")
            soft = sharp.filter(ImageFilter.GaussianBlur(1.2))
            soft.save(source / "001.png")
            soft.save(source / "002.png")
            output = root / "report"
            result = subprocess.run(
                [sys.executable, str(SCRIPT), "--analyze", str(source),
                 "--out", str(output), "--mark-at", "1"],
                capture_output=True, text=True, timeout=30,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            events = [json.loads(line) for line in (output / "events.jsonl").read_text().splitlines()]
            self.assertIn("blur_candidate", [event["kind"] for event in events])
            self.assertIn("manual_mark", [event["kind"] for event in events])
            self.assertTrue((output / "report.html").is_file())
            self.assertGreaterEqual(len(list(output.rglob("*.png"))), 3)
            report = json.loads((output / "report.json").read_text())
            self.assertEqual(report["samples"], 3)
            self.assertTrue(report["limitations"])


if __name__ == "__main__":
    unittest.main()
