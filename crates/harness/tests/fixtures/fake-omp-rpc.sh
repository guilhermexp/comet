#!/bin/sh
set -eu

emit() { printf '%s\n' "$1"; }
# Value of a TOP-LEVEL string key.
#
# Depth-aware on purpose. This replaced a `sed` whose greedy `.*` prefix took
# the LAST match, which silently meant "the innermost key": a prompt frame
# carries `"message":{"type":"text",...}`, so `field type` answered `text`
# instead of `prompt` and the dispatch below fell through with no response.
# That only surfaced when serde_json flipped from sorted to insertion order —
# `preserve_order` reaches this crate through workspace feature unification, so
# it is on in the real app and off in `cargo test -p zeron-harness`. Key order
# is not ours to assume; parse for depth instead.
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
          # Only a string value answers; an object/number means this key is
          # not the scalar the caller asked for.
          if (substr($0, j, 1) == "\"") expect = 1
          i = j; continue
        }
        i++
      }
    }'
}
has() { case "$1" in *"$2"*) return 0 ;; *) return 1 ;; esac; }
fail_stage() { emit "{\"type\":\"notice\",\"level\":\"error\",\"message\":\"fixture failed at $1\"}"; exit "$2"; }
respond() {
  line=$1
  data=$2
  emit "{\"type\":\"response\",\"id\":\"$(field id "$line")\",\"command\":\"$(field type "$line")\",\"success\":true,\"data\":$data}"
}

scenario=${FAKE_OMP_SCENARIO:-normal}
if [ "$scenario" = "require-system-prompt" ]; then
  append=
  while [ "$#" -gt 0 ]; do
    if [ "$1" = "--append-system-prompt" ]; then
      shift
      [ "$#" -gt 0 ] || exit 30
      append=$1
      break
    fi
    shift
  done
  has "$append" "# Orchestrator Control" || exit 31
  has "$append" "Communication:" || exit 32
  has "$append" "Operational boundaries:" || exit 33
fi
if [ "$scenario" = "reject-system-prompt" ]; then
  for arg in "$@"; do
    [ "$arg" != "--append-system-prompt" ] || exit 34
  done
fi
if [ "$scenario" = "require-skill-scope" ]; then
  overlay=
  while [ "$#" -gt 0 ]; do
    if [ "$1" = "--config" ]; then
      shift
      [ "$#" -gt 0 ] || exit 40
      overlay=$1
      break
    fi
    shift
  done
  [ -n "$overlay" ] || exit 41
  [ -f "$overlay" ] || exit 42
  body=$(cat "$overlay")
  has "$body" "enableClaudeUser: false" || exit 43
  has "$body" "enableAgentsUser: false" || exit 44
  has "$body" "enableCodexUser: false" || exit 45
  has "$body" "enablePiUser: false" || exit 46
fi
[ -z "${FAKE_OMP_PID_FILE:-}" ] || printf '%s\n' "$$" > "$FAKE_OMP_PID_FILE"
if [ "$scenario" = "early-exit" ]; then
  exit 9
fi
if [ "$scenario" = "stderr-crash" ]; then
  printf 'omp: no credentials for anthropic; run `omp login`\n' >&2
  exit 7
fi

emit '{"type":"ready","protocolVersion":1,"supportedProtocolVersions":[1,2]}'

if [ "$scenario" = "oversized-no-newline" ]; then
  dd if=/dev/zero bs=1048576 count=9 2>/dev/null | tr '\000' x
  sleep 5
  exit 0
fi

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
      if [ "$scenario" = "missing-session" ]; then
        respond "$line" '{"thinkingLevel":"high","model":{"provider":"openai-codex","id":"gpt-5.6-sol","name":"GPT-5.6 Sol","reasoning":true}}'
      else
        respond "$line" '{"sessionId":"s-1","sessionFile":"/tmp/omp-session.jsonl","thinkingLevel":"high","model":{"provider":"openai-codex","id":"gpt-5.6-sol","name":"GPT-5.6 Sol","reasoning":true},"contextUsage":{"tokens":392000,"contextWindow":828000,"percent":47.34},"dumpTools":[{"name":"bash","description":"Run commands","parameters":{"type":"object"}}]}'
      fi
      ;;
    get_available_models)
      respond "$line" '{"models":[{"provider":"anthropic","id":"shared","name":"Claude Shared","reasoning":false},{"provider":"openai-codex","id":"gpt-5.6-sol","name":"GPT-5.6 Sol","reasoning":true,"contextWindow":400000},{"provider":"openai-codex","id":"shared","name":"Codex Shared","reasoning":true}]}'
      ;;
    get_available_commands)
      respond "$line" '{"commands":[{"name":"model","description":"Select model","input":{"hint":"provider/model"}},{"name":"compact","description":"Compact context"}]}'
      ;;
    set_subagent_subscription)
      respond "$line" '{"level":"events"}'
      ;;
    switch_session)
      case "$line" in *'"sessionPath":"/tmp/omp-session.jsonl"'*) respond "$line" '{"cancelled":false}' ;; *) exit 22 ;; esac
      ;;
    set_host_tools)
      respond "$line" '{"toolNames":["workers"]}'
      ;;
    set_model)
      if has "$line" '"provider":"openai-codex"' && has "$line" '"modelId":"gpt-5.6-sol"'; then
        respond "$line" '{"provider":"openai-codex","id":"gpt-5.6-sol","reasoning":true}'
      else
        exit 23
      fi
      ;;
    set_thinking_level)
      case "$line" in *'"level":"high"'*) respond "$line" '{}' ;; *) exit 24 ;; esac
      ;;
    steer)
      respond "$line" '{}'
      ;;
    abort)
      respond "$line" '{}'
      ;;
    prompt)
      respond "$line" '{"agentInvoked":true}'
      if [ "$scenario" = "full-run" ]; then
        emit '{"type":"agent_start"}'
        emit '{"type":"message_update","assistantMessageEvent":{"type":"text_delta","delta":"hello"}}'
        emit '{"type":"message_update","assistantMessageEvent":{"type":"thinking_delta","delta":"checking"}}'
        emit '{"type":"extension_ui_request","id":"question-1","method":"confirm","title":"Continue?","message":"Run the checks?"}'
        queued_steer=
        while read -r answer; do
          if has "$answer" '"type":"extension_ui_response"' && has "$answer" '"id":"question-1"' && has "$answer" '"confirmed":true'; then
            break
          fi
          if has "$answer" '"type":"steer"' && has "$answer" '"message":"next"'; then
            queued_steer=$answer
          else
            fail_stage question 25
          fi
        done
        emit '{"type":"host_tool_call","id":"host-1","toolCallId":"workers-1","toolName":"workers","arguments":{"action":"help"}}'
        while read -r host_result; do
          if has "$host_result" '"type":"host_tool_result"' && has "$host_result" '"id":"host-1"' && has "$host_result" '"isError":false'; then
            break
          fi
          if has "$host_result" '"type":"steer"' && has "$host_result" '"message":"next"'; then
            queued_steer=$host_result
          else
            fail_stage host 26
          fi
        done
        emit '{"type":"tool_execution_start","toolCallId":"tool-1","toolName":"bash","args":{"command":"cargo test"}}'
        emit '{"type":"tool_execution_end","toolCallId":"tool-1","toolName":"bash","result":{"content":[{"type":"text","text":"ok"}]},"isError":false}'
        if [ -n "$queued_steer" ]; then steer=$queued_steer; else read -r steer; fi
        if has "$steer" '"type":"steer"' && has "$steer" '"message":"next"'; then
          respond "$steer" '{}'
        else
          fail_stage steer 27
        fi
        emit '{"type":"message_update","assistantMessageEvent":{"type":"text_delta","delta":" after steer"}}'
        emit '{"type":"agent_end","isTerminal":false,"messages":[]}'
        emit '{"type":"agent_start"}'
        emit '{"type":"message_update","assistantMessageEvent":{"type":"text_delta","delta":" resumed"}}'
        emit '{"type":"agent_end","messages":[]}'
      elif [ "$scenario" = "require-system-prompt" ]; then
        emit '{"type":"agent_end","messages":[]}'
      elif [ "$scenario" = "reject-system-prompt" ]; then
        emit '{"type":"agent_end","messages":[]}'
      elif [ "$scenario" = "require-skill-scope" ]; then
        emit '{"type":"agent_end","messages":[]}'
      elif [ "$scenario" = "provider-error" ]; then
        emit '{"type":"agent_end","messages":[{"role":"assistant","stopReason":"error","errorMessage":"provider failed"}]}'
      elif [ "$scenario" = "interactive-cancel" ]; then
        emit '{"type":"extension_ui_request","id":"question-pending","method":"input","title":"Value","placeholder":"Type"}'
        emit '{"type":"extension_ui_request","id":"cancel-1","method":"cancel","targetId":"question-pending"}'
        emit '{"type":"message_update","assistantMessageEvent":{"type":"text_delta","delta":"after cancel"}}'
        emit '{"type":"agent_end","messages":[]}'
      elif [ "$scenario" = "interactive-timeout" ]; then
        emit '{"type":"extension_ui_request","id":"question-timeout","method":"input","title":"Value","placeholder":"Type","timeout":25}'
        read -r timeout_response
        if has "$timeout_response" '"type":"extension_ui_response"' && has "$timeout_response" '"id":"question-timeout"' && has "$timeout_response" '"cancelled":true' && has "$timeout_response" '"timedOut":true'; then
          emit '{"type":"message_update","assistantMessageEvent":{"type":"text_delta","delta":"after timeout"}}'
          emit '{"type":"agent_end","messages":[]}'
        else
          fail_stage timeout 28
        fi
      elif [ "$scenario" = "workers-cancel" ]; then
        emit '{"type":"host_tool_call","id":"host-hang","toolCallId":"workers-hang","toolName":"workers","arguments":{"action":"hang"}}'
        emit '{"type":"host_tool_cancel","id":"cancel-host","targetId":"host-hang"}'
        emit '{"type":"message_update","assistantMessageEvent":{"type":"text_delta","delta":"after workers cancel"}}'
        emit '{"type":"agent_end","messages":[]}'
      elif [ "$scenario" = "workers-oversized" ]; then
        emit '{"type":"host_tool_call","id":"host-large","toolCallId":"workers-large","toolName":"workers","arguments":{"action":"oversized"}}'
        read -r host_result
        if has "$host_result" '"type":"host_tool_result"' && has "$host_result" '"id":"host-large"' && has "$host_result" '"isError":true'; then
          emit '{"type":"message_update","assistantMessageEvent":{"type":"text_delta","delta":"after oversized workers result"}}'
          emit '{"type":"agent_end","messages":[]}'
        else
          fail_stage workers_oversized 29
        fi
      elif [ "$scenario" = "wait" ]; then
        sleep 60
      fi
      ;;
    *)
      respond "$line" '{}'
      ;;
  esac
done
