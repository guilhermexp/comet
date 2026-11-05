#!/usr/bin/env python3
"""Metadata-only summary peer. No provider; log counts prove read-only input."""
import json, os, sys
VERSION = "2.1.318"
if "--version" in sys.argv:
    print(VERSION + " (Claude Code)")
    sys.exit(0)

def emit(frame):
    print(json.dumps(frame), flush=True)

def result():
    emit({"type":"result","subtype":"success","result":"synthetic completion","num_turns":1,"duration_api_ms":0,"usage":{"input_tokens":1,"output_tokens":1},"session_id":"context-session"})

prompts = 0
for line in sys.stdin:
    frame = json.loads(line)
    request = frame.get("request", {})
    subtype = request.get("subtype", "")
    path = os.environ.get("COMET_CONTEXT_FIXTURE_LOG")
    if path:
        with open(path,"a") as log:
            log.write(json.dumps({"type":frame.get("type"),"subtype":subtype,"detail":request.get("detail")})+"\n")
    if frame.get("type") == "control_request":
        if subtype == "get_context_usage":
            assert request.get("detail") == "summary", "full inspection could call a provider"
            if os.environ.get("COMET_CONTEXT_FIXTURE_MODE") == "exit":
                sys.exit(0)
            if os.environ.get("COMET_CONTEXT_FIXTURE_MODE") == "timeout":
                continue
            # A foreign correlated reply must never complete this request.
            emit({"type":"control_response","response":{"subtype":"success","request_id":"zeron_context_foreign","response":{"totalTokens":999}}})
            emit({"type":"control_response","response":{"subtype":"success","request_id":frame["request_id"],"response":{"categories":[{"id":"messages","name":"Messages","tokens":42,"kind":"used"}],"totalTokens":42,"maxTokens":1000,"model":"synthetic","apiUsage":None,"mcpTools":[],"memoryFiles":[]}}})
        elif subtype == "interrupt":
            sys.exit(0)
        elif subtype == "set_model" and os.environ.get("COMET_CONTEXT_FIXTURE_MODE") == "refuse-model":
            emit({"type":"control_response","response":{"subtype":"error","request_id":frame["request_id"],"error":"synthetic refusal"}})
        else:
            emit({"type":"control_response","response":{"subtype":"success","request_id":frame["request_id"],"response":{}}})
    elif frame.get("type") == "user":
        prompts += 1
        if prompts > 1 and os.environ.get("COMET_CONTEXT_FIXTURE_MODE") != "settle":
            emit(frame)  # Native confirmation that the next command was taken up.
        emit({"type":"system","subtype":"init","session_id":"context-session","model":"synthetic","cwd":"/tmp","tools":[]})
        emit({"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"active"}}})
        if os.environ.get("COMET_CONTEXT_FIXTURE_MODE") not in ("active", "settle") or (os.environ.get("COMET_CONTEXT_FIXTURE_MODE") == "settle" and prompts == 2):
            result()
