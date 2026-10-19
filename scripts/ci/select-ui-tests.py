#!/usr/bin/env python3
"""Select UI and CI test lanes based on changed files and workspace dependency manifests.

Used by .github/workflows/ui-tests.yml to filter test jobs and macOS test steps
on pull requests and main pushes.
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
import tomllib
from pathlib import Path
from typing import Dict, Iterable, Optional, Set

REPO_ROOT = Path(__file__).resolve().parent.parent.parent

CANONICAL_TARGETS = [
    "ios",
    "ui",
    "core",
    "linux_browser",
    "macos",
    "macos_app_check",
    "macos_geometry",
    "macos_notify",
    "macos_attachments",
    "macos_browser",
    "macos_preview",
    "macos_gpui",
]

CORE_PACKAGES = {
    "zeron-harness",
    "zeron-engine",
    "zeron-sync",
    "zeron-update",
    "zeron-doc",
    "zeron-preview",
    "zeron-visual",
    "zeron-visual-gpu",
    "zatex",
}

MOBILE_PACKAGES = {
    "zeron-mobile",
    "zeron-client",
    "zeron-text",
    "zeron-markdown",
}

IGNORED_DOC_FILES = {
    ".gitignore",
    ".gitattributes",
    "LICENSE",
    "LICENSE-APACHE",
    "LICENSE-MIT",
    "README.md",
    "AGENTS.md",
    "PATCHES.md",
}

GLOBAL_TRIGGER_FILES = {
    "Cargo.lock",
    "Cargo.toml",
    "rust-toolchain.toml",
    ".github/workflows/ui-tests.yml",
    "scripts/ci/select-ui-tests.py",
    "scripts/ci/test_select_ui_tests.py",
}

EXACT_FILE_TARGETS: Dict[str, Set[str]] = {
    "scripts/run-macos-browser-fixture.sh": {"macos_browser", "macos_preview", "macos"},
    "scripts/test-linux-browser.sh": {"linux_browser"},
    "scripts/test-browser-storage.py": {"macos_browser", "linux_browser", "macos"},
    "scripts/ci/test-core.sh": {"core"},
    "apps/zeron/src/fixtures/browser.rs": {"macos_browser", "linux_browser", "macos", "ui"},
    "apps/zeron/src/fixtures/preview.rs": {"macos_preview", "macos", "ui"},
    "dist/macos/Info.plist": {"macos_browser", "macos_preview", "macos_app_check", "macos"},
    "crates/ui/src/notify.rs": {"macos_notify", "macos", "ui"},
    "crates/ui/src/changes.rs": {"macos_geometry", "macos", "ui"},
}

PREFIX_TARGETS = [
    ("apps/ios/", {"ios"}),
    ("scripts/ios/", {"ios"}),
    ("dist/macos/", {"macos_app_check", "macos"}),
    ("edge/", {"macos_preview", "macos"}),
    (".github/actions/linux-ci/", {"core", "ui", "linux_browser"}),
    ("scripts/ci/", {"core", "ui", "linux_browser"}),
    ("apps/zeron/", {"macos_app_check", "macos", "ui"}),
]

ATTACHMENT_FILES = {
    "crates/ui/src/attachments.rs",
    "crates/ui/src/composer.rs",
    "crates/ui/src/queue.rs",
    "crates/ui/src/transcript.rs",
    "crates/ui/src/shell/chat_dropzone.rs",
}


def all_targets(value: bool = True) -> Dict[str, bool]:
    return {target: value for target in CANONICAL_TARGETS}


def is_ignored_doc(path_str: str) -> bool:
    norm = Path(path_str).as_posix()
    if norm in IGNORED_DOC_FILES:
        return True
    if norm.startswith("docs/") or norm.startswith("doc/") or norm.startswith(".github/ISSUE_TEMPLATE/"):
        return True
    if norm.endswith(".md") and not (norm.startswith("crates/") or norm.startswith("apps/")):
        return True
    return False


class WorkspaceGraph:
    def __init__(self, root_dir: Path):
        self.root_dir = root_dir.resolve()
        self.package_to_dir: Dict[str, Path] = {}
        self.dir_to_package: Dict[str, str] = {}
        self.direct_deps: Dict[str, Set[str]] = {}
        self.reverse_deps: Dict[str, Set[str]] = {}
        self._load()

    def _load(self) -> None:
        root_cargo = self.root_dir / "Cargo.toml"
        if not root_cargo.is_file():
            raise FileNotFoundError(f"Root Cargo.toml not found at {root_cargo}")

        with open(root_cargo, "rb") as f:
            root_data = tomllib.load(f)

        ws_deps_paths: Dict[str, Path] = {}
        for name, spec in root_data.get("workspace", {}).get("dependencies", {}).items():
            if isinstance(spec, dict) and "path" in spec:
                ws_deps_paths[name] = (self.root_dir / spec["path"]).resolve()

        for patch_dict in root_data.get("patch", {}).values():
            if isinstance(patch_dict, dict):
                for name, spec in patch_dict.items():
                    if isinstance(spec, dict) and "path" in spec:
                        ws_deps_paths[name] = (self.root_dir / spec["path"]).resolve()

        manifest_paths = []
        for sroot in [
            self.root_dir / "crates",
            self.root_dir / "apps",
            self.root_dir / "third_party" / "zui" / "crates",
            self.root_dir / "third_party" / "unpeel" / "crates",
        ]:
            if sroot.is_dir():
                manifest_paths.extend(sroot.rglob("Cargo.toml"))

        pkg_manifests = {}
        for manifest in manifest_paths:
            with open(manifest, "rb") as f:
                data = tomllib.load(f)
            pkg_name = data.get("package", {}).get("name")
            if pkg_name:
                pkg_dir = manifest.parent.resolve()
                self.package_to_dir[pkg_name] = pkg_dir
                rel_dir = str(pkg_dir.relative_to(self.root_dir))
                self.dir_to_package[rel_dir] = pkg_name
                pkg_manifests[pkg_name] = (pkg_dir, data)

        for pkg_name, (pkg_dir, data) in pkg_manifests.items():
            deps: Set[str] = set()
            tables = [
                data.get("dependencies", {}),
                data.get("dev-dependencies", {}),
                data.get("build-dependencies", {}),
            ]
            for target_val in data.get("target", {}).values():
                if isinstance(target_val, dict):
                    tables.extend([
                        target_val.get("dependencies", {}),
                        target_val.get("dev-dependencies", {}),
                        target_val.get("build-dependencies", {}),
                    ])

            for table in tables:
                if not isinstance(table, dict):
                    continue
                for dep_name, dep_spec in table.items():
                    if isinstance(dep_spec, dict):
                        if "path" in dep_spec:
                            target_path = (pkg_dir / dep_spec["path"]).resolve()
                            for cname, cdir in self.package_to_dir.items():
                                if cdir == target_path:
                                    deps.add(cname)
                                    break
                        elif dep_spec.get("workspace") is True:
                            if dep_name in ws_deps_paths:
                                target_path = ws_deps_paths[dep_name]
                                for cname, cdir in self.package_to_dir.items():
                                    if cdir == target_path:
                                        deps.add(cname)
                                        break
                            elif dep_name in self.package_to_dir:
                                deps.add(dep_name)
                    elif dep_name in self.package_to_dir:
                        deps.add(dep_name)

            self.direct_deps[pkg_name] = deps

        for pkg in self.package_to_dir:
            self.reverse_deps[pkg] = set()
        for src_pkg, deps in self.direct_deps.items():
            for dep in deps:
                if dep in self.reverse_deps:
                    self.reverse_deps[dep].add(src_pkg)

    def transitive_reverse_deps(self, pkg_name: str) -> Set[str]:
        visited: Set[str] = set()
        queue = [pkg_name]
        while queue:
            cur = queue.pop(0)
            for parent in self.reverse_deps.get(cur, set()):
                if parent not in visited:
                    visited.add(parent)
                    queue.append(parent)
        return visited

    def find_package_for_file(self, rel_path: str) -> Optional[str]:
        parts = Path(rel_path).parts
        best_match: Optional[str] = None
        best_len = 0
        for dir_str, pkg in self.dir_to_package.items():
            dir_parts = Path(dir_str).parts
            if len(parts) >= len(dir_parts) and parts[: len(dir_parts)] == dir_parts:
                if len(dir_parts) > best_len:
                    best_len = len(dir_parts)
                    best_match = pkg
        return best_match


def parse_diff_nul(data: str) -> Set[str]:
    entries = data.split("\0")
    files: Set[str] = set()
    idx = 0
    total = len(entries)
    while idx < total:
        status = entries[idx]
        if not status:
            idx += 1
            continue
        if status[0] in ("R", "C"):
            if idx + 2 < total:
                if entries[idx + 1]:
                    files.add(entries[idx + 1])
                if entries[idx + 2]:
                    files.add(entries[idx + 2])
                idx += 3
            else:
                break
        else:
            if idx + 1 < total:
                if entries[idx + 1]:
                    files.add(entries[idx + 1])
                idx += 2
            else:
                break
    return files


def get_git_diff_files(base: str, head: str, cwd: Path, is_pr: bool = False) -> Set[str]:
    diff_base = base
    if is_pr:
        mb = subprocess.run(
            ["git", "merge-base", base, head],
            cwd=cwd,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )
        if mb.returncode == 0 and mb.stdout.strip():
            diff_base = mb.stdout.strip()
    cmd = ["git", "diff", "--name-status", "-z", "-M", diff_base, head]
    proc = subprocess.run(cmd, cwd=cwd, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    if proc.returncode != 0:
        raise RuntimeError(f"git diff failed (code {proc.returncode}): {proc.stderr}")
    return parse_diff_nul(proc.stdout)


def classify_changes(changed_files: Iterable[str], root_dir: Path) -> Dict[str, bool]:
    files = [Path(f).as_posix() for f in changed_files if f]
    if not files:
        return all_targets(False)

    significant_files = [f for f in files if not is_ignored_doc(f)]
    if not significant_files:
        return all_targets(False)

    for f in significant_files:
        if (
            f in GLOBAL_TRIGGER_FILES
            or f.startswith(".cargo/")
            or f.startswith("third_party/zui/")
            or Path(f).name == "Cargo.toml"
        ):
            return all_targets(True)

    try:
        graph = WorkspaceGraph(root_dir)
    except Exception as e:
        sys.stderr.write(f"Workspace manifest parse error: {e}; falling back to all targets\n")
        return all_targets(True)

    active_flags: Set[str] = set()
    shared_native = False

    for f in significant_files:
        if f in EXACT_FILE_TARGETS:
            active_flags.update(EXACT_FILE_TARGETS[f])
            continue

        matched_prefix = False
        for prefix, targets in PREFIX_TARGETS:
            if f.startswith(prefix):
                active_flags.update(targets)
                matched_prefix = True
                break
        if matched_prefix:
            continue

        if f.startswith("third_party/unpeel/"):
            active_flags.update({"core", "ui"})
            shared_native = True
            continue

        if f.startswith("crates/ui/"):
            active_flags.add("ui")
            if f in ATTACHMENT_FILES:
                active_flags.update({"macos_attachments", "macos"})
            elif f.startswith("crates/ui/src/browser/"):
                active_flags.update({"macos_browser", "linux_browser", "macos"})
            elif f.startswith("crates/ui/src/file_preview/"):
                active_flags.update({"macos_preview", "macos"})
            else:
                shared_native = True
            continue

        pkg = graph.find_package_for_file(f)
        if pkg:
            affected = {pkg} | graph.transitive_reverse_deps(pkg)
            if affected & CORE_PACKAGES:
                active_flags.add("core")
            if affected & MOBILE_PACKAGES:
                active_flags.add("ios")
            if "zeron-ui" in affected:
                active_flags.add("ui")
                shared_native = True
            if "zeron" in affected:
                active_flags.update({"macos_app_check", "macos"})
            if "zeron-preview" in affected:
                active_flags.update({"macos_preview", "macos"})
        else:
            return all_targets(True)

    if shared_native:
        active_flags.update({
            "macos_app_check",
            "macos_geometry",
            "macos_notify",
            "macos_attachments",
            "macos_browser",
            "macos_preview",
            "macos_gpui",
            "linux_browser",
            "macos",
        })

    result = {target: (target in active_flags) for target in CANONICAL_TARGETS}
    result["macos"] = any(
        result[t]
        for t in [
            "macos_app_check",
            "macos_geometry",
            "macos_notify",
            "macos_attachments",
            "macos_browser",
            "macos_preview",
            "macos_gpui",
        ]
    )
    return result


def main() -> None:
    parser = argparse.ArgumentParser(description="Select UI test lanes based on changed files.")
    parser.add_argument("--all", action="store_true", help="Force selection of all targets")
    parser.add_argument("--pr", action="store_true", help="Diff PR head against merge-base with base")
    parser.add_argument("--base", type=str, help="Base git commit SHA for git diff")
    parser.add_argument("--head", type=str, default="HEAD", help="Head git commit SHA (default: HEAD)")
    parser.add_argument("--files", nargs="*", help="List of changed file paths")
    parser.add_argument("--root", type=str, help="Repository root path")
    parser.add_argument("--github-output", type=str, help="Path to $GITHUB_OUTPUT file")
    parser.add_argument("--json", action="store_true", help="Output results as JSON")

    args = parser.parse_args()
    root_dir = Path(args.root).resolve() if args.root else REPO_ROOT

    if args.all:
        results = all_targets(True)
    elif args.files is not None and len(args.files) > 0:
        results = classify_changes(args.files, root_dir)
    elif args.base:
        try:
            changed = get_git_diff_files(args.base, args.head, root_dir, is_pr=args.pr)
            results = classify_changes(changed, root_dir)
        except Exception as e:
            sys.stderr.write(f"git diff failed ({e}); falling back to all targets\n")
            results = all_targets(True)
    else:
        results = all_targets(True)

    if args.github_output:
        with open(Path(args.github_output), "a", encoding="utf-8") as f:
            for k, v in results.items():
                f.write(f"{k}={str(v).lower()}\n")

    if args.json:
        print(json.dumps(results, indent=2))
    elif not args.github_output:
        for k, v in results.items():
            print(f"{k}={str(v).lower()}")


if __name__ == "__main__":
    main()
