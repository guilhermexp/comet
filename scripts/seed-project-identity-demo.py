#!/usr/bin/env python3
"""Create an isolated legacy Worker catalog for project identity native QA.

Usage: seed-project-identity-demo.py --home /tmp/comet-identity-demo
Then launch with UNPEEL_HOME equal to that directory. Re-run with
--remove-observed-checkout only after the app has observed the initial catalog.
No real profile or pre-existing repository is modified.
"""
import argparse
import json
import subprocess
import time
from pathlib import Path


def git(path: Path, *args: str) -> None:
    subprocess.run(["git", "-C", str(path), *args], check=True, capture_output=True)


def write_json(path: Path, value: object) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2) + "\n", encoding="utf-8")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--home", type=Path, required=True)
    parser.add_argument("--remove-observed-checkout", action="store_true")
    args = parser.parse_args()
    home = args.home.expanduser().resolve()
    marker = home / ".comet-project-identity-demo"
    repo = home / "repositories" / "Comet Demo"
    if args.remove_observed_checkout:
        if not marker.is_file():
            parser.error("Only a profile created by this fixture can be changed")
        git(repo, "worktree", "remove", str(home / "repositories" / "observed-checkout"))
        print("Observed checkout removed; refresh/restart the demo to verify retained membership.")
        return
    if home.exists() and any(home.iterdir()):
        parser.error("Choose a new empty directory; existing profiles are never overwritten")
    home.mkdir(parents=True, exist_ok=True)
    home.chmod(0o700)
    projects = []
    def repository(name: str) -> Path:
        path = home / "repositories" / name
        path.mkdir(parents=True)
        git(path, "init", "-q", "-b", "main")
        git(path, "-c", "user.name=Demo", "-c", "user.email=demo@example.invalid", "commit", "--allow-empty", "-qm", "Initial")
        return path
    def project(key: str, name: str, path: Path) -> None:
        projects.append({"id": key, "name": name, "path": str(path), "sort_order": len(projects)})
    repository("Comet Demo")
    project("main", "Comet Demo", repo)
    for key, branch in [("external", "fix/sidebar"), ("observed-checkout", "fix/history")]:
        path = home / "repositories" / key
        git(repo, "worktree", "add", "-qb", branch, str(path))
        project(key, key, path)
    orphan_repo = repository("Principal not registered")
    child = home / "repositories" / "only-child"
    git(orphan_repo, "worktree", "add", "-qb", "feature/child", str(child))
    project("only-child", "only-child", child)
    project("missing-legacy", "Unresolved old checkout", home / "gone" / "unknown")
    projects.append({"id": "organization", "name": "Organizational group", "path": str(repo), "parent_project_id": "main", "is_folder": True})
    now = int(time.time() * 1000)
    write_json(home / "app-state.json", {"projects": projects, "presets": [], "active_tabs": {}, "pinned_sessions": {}, "fixture_unknown_key": {"preserve": True}})
    for i, item in enumerate(projects):
        if item["id"] == "organization":
            continue
        session = {"id": "worker-" + item["id"], "project_id": item["id"], "label": "Worker: " + item["name"], "command": "codex", "created_at": now - i * 60_000}
        write_json(home / "app-sessions" / session["id"] / "manifest.json", {"session": session, "cwd": item["path"], "state": "exited", "pid": None, "exit_code": 0, "updated_at": now - i * 60_000})
    marker.write_text("isolated project identity fixture\n", encoding="utf-8")
    print(home)


if __name__ == "__main__":
    main()
