#!/usr/bin/env python3
"""Seed the isolated dev demo with one Chat-linked OMP Worker."""

import argparse
import json
import time
from pathlib import Path


def write_json(path: Path, value: object) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2) + "\n", encoding="utf-8")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--home", required=True, type=Path)
    parser.add_argument("--project-path", required=True, type=Path)
    parser.add_argument("--parent-chat-id", required=True)
    args = parser.parse_args()

    now_ms = int(time.time() * 1000)
    worker_id = "demo-omp-worker"
    provider_session_id = "demo-omp-provider-session"
    transcript = args.home / "omp" / "sessions" / f"{provider_session_id}.jsonl"
    transcript.parent.mkdir(parents=True, exist_ok=True)
    transcript.write_text(
        "\n".join(
            [
                json.dumps({"type": "session", "id": provider_session_id}),
                json.dumps(
                    {
                        "type": "message",
                        "role": "assistant",
                        "model": "openai-codex/gpt-5.6-sol:high",
                        "usage": {"total_tokens": 42_100},
                    }
                ),
            ]
        )
        + "\n",
        encoding="utf-8",
    )
    transcript = transcript.resolve()

    write_json(
        args.home / "app-state.json",
        {
            "projects": [
                {
                    "id": "demo-workers-project",
                    "name": "Zeron Demo",
                    "path": str(args.project_path.resolve()),
                    "sort_order": 0,
                    "is_folder": False,
                }
            ],
            "active_project_id": "demo-workers-project",
            "presets": [],
            "active_tabs": {},
            "pinned_sessions": {},
            "comet_worker_parent_notifications": {
                worker_id: {
                    "parent_chat_id": args.parent_chat_id,
                    "registered_at_unix_ms": now_ms,
                }
            },
        },
    )
    session_dir = args.home / "app-sessions" / worker_id
    write_json(
        session_dir / "manifest.json",
        {
            "session": {
                "id": worker_id,
                "project_id": "demo-workers-project",
                "label": "Implement OMP telemetry",
                "command": "omp",
                "created_at": now_ms,
            },
            "cwd": str(args.project_path.resolve()),
            "state": "running",
            "pid": None,
            "exit_code": None,
            "provider_session_id": provider_session_id,
            "provider_transcript_path": str(transcript),
            "runtime_launch_generation": 1,
            "heartbeat_at": now_ms,
            "updated_at": now_ms,
        },
    )
    write_json(
        session_dir / "session-telemetry.json",
        {
            "providerSessionId": provider_session_id,
            "providerTranscriptPath": str(transcript),
            "totalTokens": 258_700,
            "models": [
                {
                    "model": "openai-codex/gpt-5.6-sol:high",
                    "totalTokens": 42_100,
                    "active": True,
                },
                {
                    "model": "anthropic/claude-opus-4.1",
                    "totalTokens": 216_600,
                    "active": False,
                },
            ],
        },
    )
    write_json(
        args.home / "activity-state.json",
        {"sessions": {worker_id: {"activity_status": "working", "unread": False}}},
    )


if __name__ == "__main__":
    main()
