#!/bin/sh

emit() { printf '%s\n' "$1"; }
rid() { printf '%s' "$1" | sed 's/.*"id":\([0-9]*\).*/\1/'; }
has() { case "$1" in *"$2"*) return 0 ;; *) return 1 ;; esac; }

read -r line || exit 1
has "$line" '"method":"initialize"' || exit 1
emit "{\"id\":$(rid "$line"),\"result\":{\"protocolVersion\":1,\"agentCapabilities\":{\"loadSession\":true}}}"

read -r line || exit 1
if has "$line" '"method":"session/load"'; then
  SID="workers-mcp-resumed"
elif has "$line" '"method":"session/new"'; then
  SID="workers-mcp-session"
else
  exit 1
fi
has "$line" '"name":"comet-workers"' || exit 1
has "$line" '"args":["__workers_mcp__"]' || exit 1
has "$line" '"name":"COMET_WORKERS_CONTROLLER","value":"1"' || exit 1
if has "$line" '"method":"session/load"'; then
  emit "{\"id\":$(rid "$line"),\"result\":{}}"
else
  emit "{\"id\":$(rid "$line"),\"result\":{\"sessionId\":\"workers-mcp-session\"}}"
fi

read -r line || exit 1
has "$line" '"method":"session/prompt"' || exit 1
emit "{\"method\":\"session/update\",\"params\":{\"sessionId\":\"$SID\",\"update\":{\"sessionUpdate\":\"agent_message_chunk\",\"content\":{\"type\":\"text\",\"text\":\"workers mcp configured\"}}}}"
emit "{\"id\":$(rid "$line"),\"result\":{\"stopReason\":\"end_turn\"}}"
