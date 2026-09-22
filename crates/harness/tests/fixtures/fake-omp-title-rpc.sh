#!/bin/sh
set -eu

emit() { printf '%s\n' "$1"; }
log() {
  if [ -n "${FAKE_OMP_TITLE_LOG:-}" ]; then
    printf '%s\n' "$1" >> "$FAKE_OMP_TITLE_LOG"
  fi
}
field() {
  printf '%s' "$2" | awk -v key="$1" '
    {
      n = length($0); depth = 0; i = 1
      while (i <= n) {
        c = substr($0, i, 1)
        if (c == "{" || c == "[") { depth++; i++; continue }
        if (c == "}" || c == "]") { depth--; i++; continue }
        if (c == "\"") {
          j = i + 1; str = ""
          while (j <= n) {
            d = substr($0, j, 1)
            if (d == "\\") { str = str substr($0, j + 1, 1); j += 2; continue }
            if (d == "\"") break
            str = str d; j++
          }
          i = j + 1
          if (depth == 1) {
            if (expect) { print str; exit }
            pend = str
          }
          continue
        }
        if (c == ":" && depth == 1 && pend == key) {
          j = i + 1
          while (j <= n && substr($0, j, 1) == " ") j++
          if (substr($0, j, 1) == "\"") expect = 1
          i = j; continue
        }
        i++
      }
    }'
}
respond() {
  line=$1
  data=$2
  emit "{\"type\":\"response\",\"id\":\"$(field id "$line")\",\"command\":\"$(field type "$line")\",\"success\":true,\"data\":$data}"
}

scenario=${FAKE_OMP_TITLE_SCENARIO:-generate}
[ -z "${FAKE_OMP_TITLE_PID_FILE:-}" ] || printf '%s\n' "$$" > "$FAKE_OMP_TITLE_PID_FILE"

case " $* " in *" --no-session "*) exit 40 ;; esac
emit '{"type":"ready","protocolVersion":1,"supportedProtocolVersions":[1]}'

renamed=0
while IFS= read -r line; do
  log "$line"
  case "$(field type "$line")" in
    switch_session)
      case "$scenario" in
        switch-cancel) respond "$line" '{"cancelled":true}' ;;
        *) respond "$line" '{"cancelled":false}' ;;
      esac
      ;;
    get_state)
      case "$scenario" in
        existing) respond "$line" '{"sessionId":"s-title","sessionFile":"/tmp/native-title-session.jsonl","sessionName":"Existing OMP title","messageCount":1}' ;;
        generate) if [ "$renamed" -eq 1 ]; then respond "$line" '{"sessionId":"s-title","sessionFile":"/tmp/native-title-session.jsonl","sessionName":"Native OMP title","messageCount":1}'; else respond "$line" '{"sessionId":"s-title","sessionFile":"/tmp/native-title-session.jsonl","messageCount":1}'; fi ;;
        empty-burst|rename-error|switch-cancel) respond "$line" '{"sessionId":"s-title","sessionFile":"/tmp/native-title-session.jsonl","messageCount":1}' ;;
        *) respond "$line" '{"sessionId":"s-title","sessionFile":"/tmp/native-title-session.jsonl","messageCount":1}' ;;
      esac
      ;;
    set_model|set_thinking_level|get_available_commands|set_subagent_subscription)
      [ "$renamed" -eq 0 ] || exit 45
      respond "$line" '{}'
      ;;
    prompt)
      if [ "$(field message "$line")" != "/rename" ]; then
        [ "$renamed" -eq 0 ] || exit 41
        respond "$line" '{}'
        emit '{"type":"agent_end","isTerminal":true,"messages":[]}'
        continue
      fi
      case "$scenario" in
        rename-hang) ;;
        rename-error)
          emit "{\"type\":\"response\",\"id\":\"$(field id "$line")\",\"command\":\"prompt\",\"success\":false,\"error\":\"native rename failed\"}"
          ;;
        empty-burst)
          i=0
          while [ "$i" -lt 400 ]; do
            emit "{\"type\":\"command_output\",\"text\":\"noise-$i\"}"
            i=$((i + 1))
          done
          respond "$line" '{"agentInvoked":false}'
          renamed=1
          ;;
        switch-cancel) exit 42 ;;
        *)
          respond "$line" '{"agentInvoked":false}'
          renamed=1
          ;;
      esac
      ;;
    *) exit 43 ;;
  esac
done
