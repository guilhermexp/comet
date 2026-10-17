#!/usr/bin/env python3
"""Fake cua_repl: answers initialize, js and js_reset over stdio MCP.

`js` code drives it: "hang" never answers, "die" exits, "elicit" asks for an
elicitation first, "image" returns JPEG bytes labeled PNG. Every other code
returns the preamble plus an echo.
"""
import json
import os
import sys

JPEG_B64 = "/9j/4AAQ"  # ff d8 ff e0 00 10


def send(message):
    sys.stdout.write(json.dumps(message) + "\n")
    sys.stdout.flush()


log = os.environ.get("FAKE_CUA_LOG")
pending_elicit = None
for line in sys.stdin:
    message = json.loads(line)
    if log:
        with open(log, "a") as handle:
            handle.write(line)
    method = message.get("method")
    if method is None and message.get("id") == "elicit-1" and pending_elicit:
        send({"jsonrpc": "2.0", "id": pending_elicit, "result": {"content": [{"type": "text", "text": "elicited"}]}})
        pending_elicit = None
        continue
    if method == "initialize":
        send({"jsonrpc": "2.0", "id": message["id"], "result": {"protocolVersion": "2024-11-05", "capabilities": {}}})
    elif method == "tools/call":
        name = message["params"]["name"]
        args = message["params"].get("arguments", {})
        if name == "js_reset":
            send({"jsonrpc": "2.0", "id": message["id"], "result": {"content": []}})
            continue
        code = args.get("code", "")
        if code == "hang":
            continue
        if code == "die":
            sys.stderr.write("fake runtime died\n")
            sys.exit(3)
        if code == "elicit":
            pending_elicit = message["id"]
            send({"jsonrpc": "2.0", "id": "elicit-1", "method": "elicitation/create", "params": {}})
            continue
        if code == "image":
            content = [{"type": "image", "data": JPEG_B64, "mimeType": "image/png"}]
        else:
            content = [
                {"type": "text", "text": "## Computer Use\nfake preamble"},
                {"type": "text", "text": "echo:" + code + " pid:" + str(os.getpid())},
            ]
        send({"jsonrpc": "2.0", "id": message["id"], "result": {"content": content}})
