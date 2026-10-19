#!/usr/bin/env python3
"""Behavioral tests for scripts/ci/select-ui-tests.py.

Verifies test lane selection across isolated sub-surfaces, shared Cargo manifests,
proto, shell, GPUI, core packages, NUL renames, assets, real error fallbacks, and docs.
"""

from __future__ import annotations

import importlib.util
import shutil
import sys
import tempfile
import unittest
from pathlib import Path

SCRIPT_DIR = Path(__file__).resolve().parent
REPO_ROOT = SCRIPT_DIR.parent.parent

selector_path = SCRIPT_DIR / "select-ui-tests.py"
spec = importlib.util.spec_from_file_location("select_ui_tests", selector_path)
if spec is None or spec.loader is None:
    raise ImportError(f"Cannot load module from {selector_path}")
select_ui_tests = importlib.util.module_from_spec(spec)
sys.modules["select_ui_tests"] = select_ui_tests
spec.loader.exec_module(select_ui_tests)

all_targets = select_ui_tests.all_targets
classify_changes = select_ui_tests.classify_changes
parse_diff_nul = select_ui_tests.parse_diff_nul
get_git_diff_files = select_ui_tests.get_git_diff_files


class TestSelectUiTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.repo_root = REPO_ROOT

    def test_ios_only(self) -> None:
        """Changes strictly inside iOS app or iOS scripts only run iOS lane."""
        cases = [
            ["apps/ios/Zeron/AppDelegate.swift"],
            ["scripts/ios/build-core.sh"],
            ["crates/mobile/src/lib.rs"],
            ["crates/client/src/lib.rs"],
            ["apps/ios/Zeron.xcodeproj/project.pbxproj", "scripts/ios/build-core.sh"],
        ]
        for files in cases:
            with self.subTest(files=files):
                res = classify_changes(files, self.repo_root)
                self.assertTrue(res["ios"], f"ios should be True for {files}")
                self.assertFalse(res["ui"], f"ui should be False for {files}")
                self.assertFalse(res["core"], f"core should be False for {files}")
                self.assertFalse(res["linux_browser"], f"linux_browser should be False for {files}")
                self.assertFalse(res["macos"], f"macos should be False for {files}")
                self.assertFalse(res["macos_app_check"])
                self.assertFalse(res["macos_geometry"])
                self.assertFalse(res["macos_notify"])
                self.assertFalse(res["macos_attachments"])
                self.assertFalse(res["macos_browser"])
                self.assertFalse(res["macos_preview"])
                self.assertFalse(res["macos_gpui"])

    def test_visual_gpu_and_math_preserve_shared_core_coverage(self) -> None:
        for path in ["crates/visual-gpu/src/lib.rs", "crates/zatex/src/lib.rs"]:
            with self.subTest(path=path):
                selected = classify_changes([path], self.repo_root)
                for lane in ["core", "ios", "ui", "macos"]:
                    self.assertTrue(selected[lane], f"{path} must select {lane}")

    def test_notify_isolated(self) -> None:
        """Isolated change to crates/ui/src/notify.rs must not run browser, preview, or GPUI."""
        res = classify_changes(["crates/ui/src/notify.rs"], self.repo_root)
        self.assertTrue(res["macos_notify"])
        self.assertTrue(res["macos"])
        self.assertTrue(res["ui"])
        self.assertFalse(res["macos_browser"])
        self.assertFalse(res["macos_preview"])
        self.assertFalse(res["macos_gpui"])
        self.assertFalse(res["macos_geometry"])
        self.assertFalse(res["macos_attachments"])
        self.assertFalse(res["macos_app_check"])
        self.assertFalse(res["linux_browser"])
        self.assertFalse(res["core"])
        self.assertFalse(res["ios"])

    def test_browser(self) -> None:
        """Changes to browser-specific code trigger macOS browser and Linux browser."""
        cases = [
            ["crates/ui/src/browser/view.rs"],
            ["scripts/test-linux-browser.sh"],
            ["scripts/test-browser-storage.py"],
            ["apps/zeron/src/fixtures/browser.rs"],
        ]
        for files in cases:
            with self.subTest(files=files):
                res = classify_changes(files, self.repo_root)
                self.assertTrue(res["linux_browser"])
                self.assertFalse(res["macos_preview"])
                self.assertFalse(res["macos_gpui"])
                self.assertFalse(res["macos_notify"])
                self.assertFalse(res["macos_geometry"])
                self.assertFalse(res["core"])
                self.assertFalse(res["ios"])

    def test_preview(self) -> None:
        """Changes to preview fixture/code trigger preview fixture but not browser/GPUI/notify."""
        cases = [
            ["crates/ui/src/file_preview/preview.rs"],
            ["edge/src/server.ts"],
            ["apps/zeron/src/fixtures/preview.rs"],
        ]
        for files in cases:
            with self.subTest(files=files):
                res = classify_changes(files, self.repo_root)
                self.assertTrue(res["macos_preview"])
                self.assertTrue(res["macos"])
                self.assertFalse(res["macos_browser"])
                self.assertFalse(res["macos_gpui"])
                self.assertFalse(res["macos_notify"])
                self.assertFalse(res["macos_geometry"])
                self.assertFalse(res["macos_attachments"])
                self.assertFalse(res["linux_browser"])
                self.assertFalse(res["ios"])

    def test_shared_cargo(self) -> None:
        """Changes to global manifests or lockfiles conservatively trigger all lanes."""
        cases = [
            ["Cargo.lock"],
            ["Cargo.toml"],
            ["rust-toolchain.toml"],
            [".cargo/config.toml"],
            [".github/workflows/ui-tests.yml"],
            ["crates/ui/Cargo.toml"],
        ]
        for files in cases:
            with self.subTest(files=files):
                res = classify_changes(files, self.repo_root)
                for k, v in res.items():
                    self.assertTrue(v, f"{k} should be True for {files}")

    def test_proto(self) -> None:
        """Changes to crates/proto affect iOS, core Linux suites, and native desktop."""
        res = classify_changes(["crates/proto/src/lib.rs"], self.repo_root)
        self.assertTrue(res["ios"])
        self.assertTrue(res["core"])
        self.assertTrue(res["ui"])
        self.assertTrue(res["macos"])
        self.assertTrue(res["macos_app_check"])
        self.assertTrue(res["macos_geometry"])
        self.assertTrue(res["macos_notify"])
        self.assertTrue(res["macos_attachments"])
        self.assertTrue(res["macos_browser"])
        self.assertTrue(res["macos_preview"])
        self.assertTrue(res["macos_gpui"])
        self.assertTrue(res["linux_browser"])

    def test_shell(self) -> None:
        """Shared native UI code triggers all native surfaces and ui, but not ios or core."""
        res = classify_changes(["crates/ui/src/shell/tabs.rs"], self.repo_root)
        self.assertTrue(res["ui"])
        self.assertTrue(res["macos"])
        self.assertTrue(res["macos_app_check"])
        self.assertTrue(res["macos_geometry"])
        self.assertTrue(res["macos_notify"])
        self.assertTrue(res["macos_attachments"])
        self.assertTrue(res["macos_browser"])
        self.assertTrue(res["macos_preview"])
        self.assertTrue(res["macos_gpui"])
        self.assertTrue(res["linux_browser"])
        self.assertFalse(res["ios"])
        self.assertFalse(res["core"])

    def test_gpui(self) -> None:
        """Changes to third_party/zui (shared GPUI) conservatively trigger native surfaces and ui."""
        res = classify_changes(["third_party/zui/crates/gpui/src/lib.rs"], self.repo_root)
        self.assertTrue(res["macos_gpui"])
        self.assertTrue(res["macos"])
        self.assertTrue(res["linux_browser"])
        self.assertTrue(res["ui"])
        self.assertTrue(res["macos_app_check"])
        self.assertTrue(res["macos_browser"])
        self.assertTrue(res["macos_preview"])

    def test_core_deps(self) -> None:
        """Changes to core packages run core-tests and dependent ui suites."""
        res = classify_changes(["crates/sync/src/lib.rs"], self.repo_root)
        self.assertTrue(res["core"])
        self.assertTrue(res["ui"])

    def test_rename_nul(self) -> None:
        """NUL-delimited diff parses renames without strip and preserves both paths."""
        nul_diff = (
            "R100\0crates/ui/src/old notify with space.rs\0crates/ui/src/notify.rs\0"
            "M\0apps/ios/Main View.swift\0"
        )
        files = parse_diff_nul(nul_diff)
        self.assertIn("crates/ui/src/old notify with space.rs", files)
        self.assertIn("crates/ui/src/notify.rs", files)
        self.assertIn("apps/ios/Main View.swift", files)

        res = classify_changes(files, self.repo_root)
        self.assertTrue(res["macos_notify"])
        self.assertTrue(res["ios"])

    def test_assets_not_ignored(self) -> None:
        """Asset files in crates/ui must not be ignored and must trigger UI suites."""
        cases = [
            ["crates/ui/assets/icon.png"],
            ["crates/ui/assets/theme.svg"],
            ["crates/ui/assets/fonts/licenses/SIL.txt"],
        ]
        for files in cases:
            with self.subTest(files=files):
                res = classify_changes(files, self.repo_root)
                self.assertTrue(res["ui"], f"ui should be True for asset {files}")
                self.assertTrue(res["macos"], f"macos should be True for asset {files}")
                self.assertTrue(res["macos_app_check"])

    def test_manifest_parse_error_fallback(self) -> None:
        """Corrupt or invalid workspace manifest raises error and falls back to all targets."""
        with tempfile.TemporaryDirectory() as tmp_dir:
            tmp_root = Path(tmp_dir)
            (tmp_root / "Cargo.toml").write_text("invalid [ [ toml syntax", encoding="utf-8")
            res = classify_changes(["crates/ui/src/notify.rs"], tmp_root)
            for k, v in res.items():
                self.assertTrue(v, f"{k} should be True on manifest parse error fallback")

    def test_git_diff_error_fallback(self) -> None:
        """Non-existent git commit raises exception in get_git_diff_files."""
        with self.assertRaises(RuntimeError):
            get_git_diff_files("0000000000000000000000000000000000000000", "HEAD", self.repo_root)

    def test_docs_only(self) -> None:
        """Changes only affecting documentation or ignored files run no suites."""
        res = classify_changes(
            ["README.md", "docs/architecture.md", ".gitignore", "LICENSE"],
            self.repo_root,
        )
        for k, v in res.items():
            self.assertFalse(v, f"{k} should be False for docs-only changes")


if __name__ == "__main__":
    unittest.main()
