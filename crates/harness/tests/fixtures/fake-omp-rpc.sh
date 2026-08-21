#!/bin/sh
set -eu

emit() { printf '%s\n' "$1"; }
field() {
  key=$1
  line=$2
  printf '%s' "$line" | sed -n "s/.*\"$key\":\"\([^\"]*\)\".*/\1/p"
}
respond() {
  line=$1
  data=$2
  emit "{\"type\":\"response\",\"id\":\"$(field id "$line")\",\"command\":\"$(field type "$line")\",\"success\":true,\"data\":$data}"
}

scenario=${FAKE_OMP_SCENARIO:-normal}
if [ "$scenario" = "early-exit" ]; then
  exit 9
fi

emit '{"type":"ready","protocolVersion":1,"supportedProtocolVersions":[1,2]}'

if [ "$scenario" = "malformed" ]; then
  emit 'not-json'
  sleep 1
  exit 0
fi

if [ "$scenario" = "out-of-order" ]; then
  read -r first
  read -r second
  respond "$second" '{"models":[{"provider":"openai-codex","id":"gpt-5.6-sol","name":"GPT-5.6 Sol","reasoning":true}]}'
  emit '{"type":"available_commands_update","commands":[{"name":"model","description":"Select model"}]}'
  respond "$first" '{"sessionId":"s-1","thinkingLevel":"high"}'
fi

while IFS= read -r line; do
  case "$(field type "$line")" in
    get_state)
      respond "$line" '{"sessionId":"s-1","thinkingLevel":"high"}'
      ;;
    get_available_models)
      respond "$line" '{"models":[{"provider":"openai-codex","id":"gpt-5.6-sol","name":"GPT-5.6 Sol","reasoning":true}]}'
      ;;
    *)
      respond "$line" '{}'
      ;;
  esac
done
