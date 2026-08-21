#!/bin/sh
set -eu

[ "${COMET_WORKERS_CONTROLLER:-}" = "1" ] || exit 12
emit() { printf '%s\n' "$1"; }
rid() { printf '%s' "$1" | sed -n 's/.*"id":\([0-9]*\).*/\1/p'; }

while IFS= read -r line; do
  case "$line" in
    *'"method":"initialize"'*)
      emit "{\"jsonrpc\":\"2.0\",\"id\":$(rid "$line"),\"result\":{\"protocolVersion\":\"2024-11-05\",\"capabilities\":{\"tools\":{}},\"serverInfo\":{\"name\":\"fake-workers\",\"version\":\"1\"}}}"
      ;;
    *'"method":"tools/list"'*)
      emit "{\"jsonrpc\":\"2.0\",\"id\":$(rid "$line"),\"result\":{\"tools\":[{\"name\":\"workers\",\"description\":\"Coordinate test workers\",\"inputSchema\":{\"type\":\"object\",\"required\":[\"action\"],\"properties\":{\"action\":{\"type\":\"string\"}}}}]}}"
      ;;
    *'"method":"tools/call"'*)
      case "$line" in
        *'"action":"hang"'*) sleep 60 ;;
        *'"action":"oversized"'*)
          printf '{"jsonrpc":"2.0","id":%s,"result":{"content":[{"type":"text","text":"' "$(rid "$line")"
          dd if=/dev/zero bs=1048576 count=3 2>/dev/null | tr '\000' x
          printf '"}],"isError":false}}\n'
          ;;
        *) emit "{\"jsonrpc\":\"2.0\",\"id\":$(rid "$line"),\"result\":{\"content\":[{\"type\":\"text\",\"text\":\"worker help\"}],\"isError\":false}}" ;;
      esac
      ;;
  esac
done
