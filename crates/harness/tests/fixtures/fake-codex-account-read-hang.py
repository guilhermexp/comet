#!/usr/bin/env python3
"""Codex peer that withholds only account/read and serves normal text startup."""
import json
import pathlib
import sys


for line in sys.stdin:
    frame = json.loads(line)
    method = frame.get("method")
    with pathlib.Path("account-read-wire.jsonl").open("a") as log:
        log.write(json.dumps(frame) + "\n")

    if "id" not in frame:
        continue
    if method == "account/read":
        # The optional auth snapshot can hang on real app-server versions.
        continue

    if method == "thread/start":
        result = {"thread": {"id": "account-warmup-thread", "turns": []}}
    elif method == "turn/start":
        result = {"turn": {"id": "account-warmup-turn"}}
    else:
        result = {}
    print(json.dumps({"id": frame["id"], "result": result}), flush=True)

    if method == "turn/start":
        print(
            json.dumps(
                {
                    "method": "turn/completed",
                    "params": {
                        "threadId": "account-warmup-thread",
                        "turn": {"id": "account-warmup-turn", "status": "completed"},
                    },
                }
            ),
            flush=True,
        )
