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
      emit "{\"jsonrpc\":\"2.0\",\"id\":$(rid "$line"),\"result\":{\"content\":[{\"type\":\"text\",\"text\":\"worker help\"}],\"isError\":false}}"
      ;;
  esac
done
