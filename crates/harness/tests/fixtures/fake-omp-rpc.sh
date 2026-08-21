#!/bin/sh
set -eu

emit() { printf '%s\n' "$1"; }
field() {
  key=$1
  line=$2
  printf '%s' "$line" | sed -n "s/.*\"$key\":\"\([^\"]*\)\".*/\1/p"
}
has() { case "$1" in *"$2"*) return 0 ;; *) return 1 ;; esac; }
fail_stage() { emit "{\"type\":\"notice\",\"level\":\"error\",\"message\":\"fixture failed at $1\"}"; exit "$2"; }
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
      respond "$line" '{"sessionId":"s-1","sessionFile":"/tmp/omp-session.jsonl","thinkingLevel":"high","model":{"provider":"openai-codex","id":"gpt-5.6-sol","name":"GPT-5.6 Sol","reasoning":true},"dumpTools":[{"name":"bash","description":"Run commands","parameters":{"type":"object"}}]}'
      ;;
    get_available_models)
      respond "$line" '{"models":[{"provider":"anthropic","id":"shared","name":"Claude Shared","reasoning":false},{"provider":"openai-codex","id":"shared","name":"Codex Shared","reasoning":true},{"provider":"openai-codex","id":"gpt-5.6-sol","name":"GPT-5.6 Sol","reasoning":true,"contextWindow":400000}]}'
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
      elif [ "$scenario" = "provider-error" ]; then
        emit '{"type":"agent_end","messages":[{"role":"assistant","stopReason":"error","errorMessage":"provider failed"}]}'
      elif [ "$scenario" = "wait" ]; then
        sleep 60
      fi
      ;;
    *)
      respond "$line" '{}'
      ;;
  esac
done
