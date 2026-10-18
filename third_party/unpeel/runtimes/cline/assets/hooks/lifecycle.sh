#!/bin/bash
umask 077
SOURCE_EVENT="$1"
INPUT="${2:-$(cat)}"
TRACE_FILE="${UNPEEL_HOOK_TRACE_FILE:-$HOME/.zeron/workers/hooks/trace.log}"
UNPEEL_PORT_REGISTRY_FILE="${UNPEEL_APP_PORT_REGISTRY_FILE:-$HOME/.unpeel/app-ports}"
mkdir -p "$(dirname "$TRACE_FILE")" >/dev/null 2>&1 || true

json_escape_string() {
  printf '%s' "$1" | sed 's/\\/\\\\/g; s/"/\\"/g'
}

runtime_generation_json_field() {
  case "${UNPEEL_RUNTIME_GENERATION:-}" in
    ''|*[!0-9]*) return 0 ;;
  esac
  printf ',"unpeel_runtime_generation":%s' "$UNPEEL_RUNTIME_GENERATION"
}
task_episode_json_field() {
  _episode_dir="${UNPEEL_SESSION_DIR:-$HOME/.unpeel/app-sessions/${UNPEEL_SESSION_ID:-}}"
  _episode_file="$_episode_dir/comet-task-episode"
  [ -r "$_episode_file" ] || return 0
  _episode=$(tr -cd '0-9' < "$_episode_file" 2>/dev/null)
  case "$_episode" in ''|*[!0-9]*) return 0 ;; esac
  printf ',"comet_task_episode":%s' "$_episode"
}

json_string_value() {
  _key="$1"
  printf '%s' "$INPUT" | grep -oE "\"$_key\"[[:space:]]*:[[:space:]]*\"[^\"]*\"" | head -1 | sed 's/.*: *"\([^"]*\)".*/\1/'
}

current_unpeel_ports() {
  [ -f "$UNPEEL_PORT_REGISTRY_FILE" ] || return 1
  tr -cs '0-9' '\n' < "$UNPEEL_PORT_REGISTRY_FILE" 2>/dev/null \
    | awk 'NF && !seen[$0]++ { print }'
}

post_hook_payload() {
  _hook_payload="$1"
  _hook_session_id="$2"
  _hook_port="$3"
  _hook_query="${4:-}"
  [ -n "$_hook_port" ] || return 1
  _hook_response=$(printf '%s' "$_hook_payload" | curl -sS --noproxy '*' --max-time 2 \
    -w '\n%{http_code}' -X POST -H "Content-Type: application/json" \
    -d @- "http://127.0.0.1:$_hook_port/hook/$_hook_session_id$_hook_query" 2>/dev/null)
  _hook_curl_status=$?
  [ "$_hook_curl_status" -eq 0 ] || return 1
  _hook_status=${_hook_response##*$'\n'}
  _hook_body=${_hook_response%$'\n'$_hook_status}
  if [ "$_hook_status" = "202" ] \
    && printf '%s' "$_hook_body" | grep -qE '"ignored"[[:space:]]*:[[:space:]]*true'; then
    HOOK_INGRESS_REJECTED=1
    return 0
  fi
  case "$_hook_status" in
    2[0-9][0-9]) HOOK_INGRESS_ACCEPTED=1; return 0 ;;
  esac
  return 1
}

post_hook_payload_to_current_ports() {
  _hook_payload="$1"
  _hook_session_id="$2"
  _hook_skip_port="$3"
  _hook_any_posted=1
  for _hook_candidate_port in $(current_unpeel_ports); do
    [ -n "$_hook_candidate_port" ] || continue
    [ "$_hook_candidate_port" = "$_hook_skip_port" ] && continue
    if post_hook_payload "$_hook_payload" "$_hook_session_id" "$_hook_candidate_port"; then
      _hook_any_posted=0
    fi
  done
  return $_hook_any_posted
}

preflight_hook_payload_to_known_ports() {
  _hook_payload="$1"
  _hook_session_id="$2"
  _hook_ports="${UNPEEL_APP_PORT:-} $(current_unpeel_ports)"
  _hook_seen_ports=" "
  for _hook_candidate_port in $_hook_ports; do
    [ -n "$_hook_candidate_port" ] || continue
    case "$_hook_candidate_port" in *[!0-9]*) continue ;; esac
    case "$_hook_seen_ports" in *" $_hook_candidate_port "*) continue ;; esac
    _hook_seen_ports="$_hook_seen_ports$_hook_candidate_port "
    post_hook_payload "$_hook_payload" "$_hook_session_id" "$_hook_candidate_port" "?validate=1" || true
  done
}

record_last_hook_event() {
  [ "${HOOK_INGRESS_REJECTED:-0}" != "1" ] || return 0
  _record_event_name="$1"
  _record_tool_name="$2"
  [ -n "${UNPEEL_SESSION_ID:-}" ] || return 0
  _record_dir="${UNPEEL_SESSION_DIR:-$HOME/.unpeel/app-sessions/$UNPEEL_SESSION_ID}"
  [ -d "$_record_dir" ] || return 0

  case "$_record_event_name" in
    Start|UserPromptSubmit|Stop|StopFailure|PermissionRequest) ;;
    *) return 0 ;;
  esac

  _record_payload=$(printf '{"hook_event_name":"%s"' "$(json_escape_string "$_record_event_name")")
  if [ -n "$_record_tool_name" ]; then
    _record_payload="$_record_payload$(printf ',"tool_name":"%s"' "$(json_escape_string "$_record_tool_name")")"
  fi
  _record_payload="$_record_payload,\"unpeel_runtime_id\":\"cline\""
  _record_generation="$(runtime_generation_json_field)"
  [ -n "$_record_generation" ] && _record_payload="$_record_payload$_record_generation"
  _record_episode="$(task_episode_json_field)"
  [ -n "$_record_episode" ] && _record_payload="$_record_payload$_record_episode"
  if [ -n "$PROVIDER_SESSION_ID" ]; then
    _record_payload="$_record_payload$(printf ',"session_id":"%s"' "$(json_escape_string "$PROVIDER_SESSION_ID")")"
  fi
  if [ -n "$TRANSCRIPT_PATH" ]; then
    _record_payload="$_record_payload$(printf ',"transcript_path":"%s"' "$(json_escape_string "$TRANSCRIPT_PATH")")"
  fi
  if [ -n "$AGENT_KIND" ]; then
    _record_payload="$_record_payload$(printf ',"unpeel_agent_kind":"%s"' "$(json_escape_string "$AGENT_KIND")")"
  fi
  _record_payload="$_record_payload}"
  _record_tmp="$_record_dir/.last-hook-event.json.$$"
  if printf '%s' "$_record_payload" > "$_record_tmp" 2>/dev/null; then
    mv -f "$_record_tmp" "$_record_dir/last-hook-event.json" 2>/dev/null \
      || rm -f "$_record_tmp" 2>/dev/null || true
  fi
}

extract_cline_tool_name() {
  for _k in toolName tool_name; do
    _val="$(json_string_value "$_k" || true)"
    [ -n "$_val" ] && { printf '%s\n' "$_val"; return 0; }
  done
  _cand=$(printf '%s' "$INPUT" | grep -oE '"(tool_call|tool_result)"[[:space:]]*:[[:space:]]*\{[^}]*"name"[[:space:]]*:[[:space:]]*"[^"]*"' | head -1 | grep -oE '"name"[[:space:]]*:[[:space:]]*"[^"]*"' | sed 's/.*: *"\([^"]*\)".*/\1/')
  [ -n "$_cand" ] && { printf '%s\n' "$_cand"; return 0; }
  return 1
}

TOOL_NAME="$(extract_cline_tool_name || true)"

case "$SOURCE_EVENT" in
  # Cline 3.0.44 does not dispatch UserPromptSubmit for the initial CLI prompt.
  # TaskStart/TaskResume occur when a run actually begins, so they are the
  # reliable busy edge as well as the first place the persisted session id is
  # available.
  TaskStart|TaskResume) EVENT_TYPE="UserPromptSubmit" ;;
  UserPromptSubmit) EVENT_TYPE="UserPromptSubmit" ;;
  PreToolUse)
    if [ "$TOOL_NAME" = "ask_question" ]; then
      EVENT_TYPE="PermissionRequest"
    else
      EVENT_TYPE="Start"
    fi
    ;;
  PostToolUse)
    EVENT_TYPE="Start"
    ;;
  TaskComplete|TaskCancel|SessionShutdown) EVENT_TYPE="Stop" ;;
  TaskError) EVENT_TYPE="StopFailure" ;;
  *) exit 0 ;;
esac

PROVIDER_SESSION_ID="$(json_string_value rootSessionId || true)"
[ -n "$PROVIDER_SESSION_ID" ] || PROVIDER_SESSION_ID="$(json_string_value taskId || true)"
PROVIDER_CWD="$(json_string_value rootPath || true)"
if [ -z "$PROVIDER_CWD" ]; then
  PROVIDER_CWD="$(printf '%s' "$INPUT" | grep -oE '"workspaceRoots"[[:space:]]*:[[:space:]]*\[[[:space:]]*"[^"]*"' | head -1 | sed 's/.*\[[[:space:]]*"\([^"]*\)".*/\1/' || true)"
fi

PARENT_AGENT_ID="$(json_string_value parent_agent_id || true)"
[ -z "$PARENT_AGENT_ID" ] && PARENT_AGENT_ID="$(json_string_value parentAgentId || true)"
[ "$PARENT_AGENT_ID" = "null" ] && PARENT_AGENT_ID=""
if [ -n "$PARENT_AGENT_ID" ]; then
  # Identified subagents must never overwrite root worker lifecycle state or seed
  exit 0
fi
AGENT_KIND=""

TRANSCRIPT_PATH=""
if [ -n "$PROVIDER_SESSION_ID" ]; then
  _sessions_root="${CLINE_SESSION_DATA_DIR:-}"
  if [ -z "$_sessions_root" ]; then
    _data_root="${CLINE_DATA_DIR:-${CLINE_DIR:-$HOME/.cline}/data}"
    _sessions_root="$_data_root/sessions"
  fi
  _candidate="$_sessions_root/$PROVIDER_SESSION_ID/$PROVIDER_SESSION_ID.messages.json"
  [ -f "$_candidate" ] && TRANSCRIPT_PATH="$_candidate"
fi

PAYLOAD=$(printf '{"hook_event_name":"%s"' "$(json_escape_string "$EVENT_TYPE")")
if [ -n "$TOOL_NAME" ]; then
  PAYLOAD="$PAYLOAD$(printf ',"tool_name":"%s"' "$(json_escape_string "$TOOL_NAME")")"
fi
PAYLOAD="$PAYLOAD,\"unpeel_runtime_id\":\"cline\""
if [ -n "$PROVIDER_SESSION_ID" ]; then
  PAYLOAD="$PAYLOAD$(printf ',"session_id":"%s"' "$(json_escape_string "$PROVIDER_SESSION_ID")")"
fi
if [ -n "$TRANSCRIPT_PATH" ]; then
  PAYLOAD="$PAYLOAD$(printf ',"transcript_path":"%s"' "$(json_escape_string "$TRANSCRIPT_PATH")")"
fi
if [ -n "$AGENT_KIND" ]; then
  PAYLOAD="$PAYLOAD$(printf ',"unpeel_agent_kind":"%s"' "$(json_escape_string "$AGENT_KIND")")"
fi
PAYLOAD="$PAYLOAD$(runtime_generation_json_field)"
PAYLOAD="$PAYLOAD$(task_episode_json_field)"
PAYLOAD="$PAYLOAD}"

# Cline's hook files are global. Outside an Unpeel-hosted terminal they must be
# silent no-ops so ordinary Cline sessions keep their native behavior.
if [ -n "${UNPEEL_SESSION_ID:-}" ]; then
  HOOK_INGRESS_ACCEPTED=0
  HOOK_INGRESS_REJECTED=0
  post_to_unpeel() {
    post_hook_payload "$PAYLOAD" "$UNPEEL_SESSION_ID" "${UNPEEL_APP_PORT:-}"
    post_hook_payload_to_current_ports "$PAYLOAD" "$UNPEEL_SESSION_ID" "${UNPEEL_APP_PORT:-}"
  }
  preflight_hook_payload_to_known_ports "$PAYLOAD" "$UNPEEL_SESSION_ID"
  if [ "${HOOK_INGRESS_REJECTED:-0}" != "1" ]; then
    record_last_hook_event "$EVENT_TYPE" "$TOOL_NAME"
    if [ "${UNPEEL_HOOK_POST_SYNC:-}" = "1" ]; then
      post_to_unpeel
    else
      ( post_to_unpeel ) &
      exit 0
    fi
  fi
  printf '%s cline-hook session=%s port=%s event=%s provider_session=%s\n' \
    "$(date '+%Y-%m-%d %H:%M:%S')" \
    "$UNPEEL_SESSION_ID" \
    "${UNPEEL_APP_PORT:-}" \
    "$EVENT_TYPE" \
    "$PROVIDER_SESSION_ID" >> "$TRACE_FILE" 2>/dev/null || true
fi

exit 0
