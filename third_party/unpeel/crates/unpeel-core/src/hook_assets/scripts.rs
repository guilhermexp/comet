//! Provider-neutral hook transport shared by runtime-owned setup adapters.

pub(crate) const NOTIFY_HOOK_SCRIPT: &str = r#"#!/bin/bash
umask 077
if [ -n "$1" ]; then
  INPUT="$1"
else
  INPUT=$(cat)
fi
TRACE_FILE="${UNPEEL_HOOK_TRACE_FILE:-$HOME/.zeron/workers/hooks/trace.log}"
mkdir -p "$(dirname "$TRACE_FILE")" >/dev/null 2>&1 || true
if [ -f "$TRACE_FILE" ]; then
  _unpeel_trace_size=$(wc -c < "$TRACE_FILE" 2>/dev/null | tr -d ' ')
  if [ -n "$_unpeel_trace_size" ] && [ "$_unpeel_trace_size" -gt 10485760 ]; then
    mv -f "$TRACE_FILE" "$TRACE_FILE.1" 2>/dev/null || true
  fi
fi
UNPEEL_PORT_REGISTRY_FILE="${UNPEEL_APP_PORT_REGISTRY_FILE:-$HOME/.unpeel/app-ports}"

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

current_unpeel_ports() {
  [ -f "$UNPEEL_PORT_REGISTRY_FILE" ] || return 1
  tr -cs '0-9' '\n' < "$UNPEEL_PORT_REGISTRY_FILE" 2>/dev/null \
    | awk 'NF && !seen[$0]++ { print }'
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

json_escape_string() {
  printf '%s' "$1" | sed 's/\\/\\\\/g; s/"/\\"/g'
}

json_string_value() {
  _key="$1"
  printf '%s' "$INPUT" | grep -oE "\"$_key\"[[:space:]]*:[[:space:]]*\"[^\"]*\"" | head -1 | sed 's/.*: *"\([^"]*\)".*/\1/'
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

# Persist the last lifecycle event into the session dir so a restarted app can
# re-seed busy/attention state: hooks keep firing while no app instance is
# listening, so this file is the durable record of the final transition.
# Written atomically; preserves whitelist identity only (no raw prompts/tool args).
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
  if [ -n "$UNPEEL_RUNTIME_ID" ]; then
    _record_payload="$_record_payload$(printf ',"unpeel_runtime_id":"%s"' "$(json_escape_string "$UNPEEL_RUNTIME_ID")")"
  fi
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
  if [ -n "$AGENT_ROLE" ]; then
    _record_payload="$_record_payload$(printf ',"agent_role":"%s"' "$(json_escape_string "$AGENT_ROLE")")"
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

EVENT_TYPE=$(json_string_value hook_event_name || true)
[ -z "$EVENT_TYPE" ] && EVENT_TYPE=$(json_string_value hookEventName || true)
[ "$EVENT_TYPE" = "UserPromptSubmit" ] && EVENT_TYPE="Start"
if [ "${UNPEEL_HOOK_TRACE_VERBOSE:-}" = "1" ]; then
  TRACE_RAW="$INPUT"
else
  TRACE_RAW="<redacted; UNPEEL_HOOK_TRACE_VERBOSE=1 to log>"
fi
[ -n "$EVENT_TYPE" ] && printf '%s notify-hook session=%s port=%s event=%s raw=%s\n' \
  "$(date '+%Y-%m-%d %H:%M:%S')" \
  "${UNPEEL_SESSION_ID:-}" \
  "${UNPEEL_APP_PORT:-}" \
  "$EVENT_TYPE" \
  "$TRACE_RAW" >> "$TRACE_FILE" 2>/dev/null || true
[ -z "$EVENT_TYPE" ] && exit 0

LAST_TOOL_NAME=$(json_string_value tool_name || true)
[ -z "$LAST_TOOL_NAME" ] && LAST_TOOL_NAME=$(json_string_value toolName || true)

UNPEEL_RUNTIME_ID=$(json_string_value unpeel_runtime_id || true)
PROVIDER_SESSION_ID=$(json_string_value session_id || true)
[ -z "$PROVIDER_SESSION_ID" ] && PROVIDER_SESSION_ID=$(json_string_value chatId || true)
[ -z "$PROVIDER_SESSION_ID" ] && PROVIDER_SESSION_ID=$(json_string_value chat_id || true)
[ -z "$PROVIDER_SESSION_ID" ] && PROVIDER_SESSION_ID=$(json_string_value provider_session_id || true)
[ -z "$PROVIDER_SESSION_ID" ] && PROVIDER_SESSION_ID=$(json_string_value providerSessionID || true)
[ -z "$PROVIDER_SESSION_ID" ] && PROVIDER_SESSION_ID=$(json_string_value providerSessionId || true)
[ -z "$PROVIDER_SESSION_ID" ] && PROVIDER_SESSION_ID=$(json_string_value sessionId || true)
[ -z "$PROVIDER_SESSION_ID" ] && PROVIDER_SESSION_ID=$(json_string_value thread_id || true)
[ -z "$PROVIDER_SESSION_ID" ] && PROVIDER_SESSION_ID=$(json_string_value threadID || true)
[ -z "$PROVIDER_SESSION_ID" ] && PROVIDER_SESSION_ID=$(json_string_value threadId || true)
[ -z "$PROVIDER_SESSION_ID" ] && PROVIDER_SESSION_ID=$(json_string_value conversation_id || true)
[ -z "$PROVIDER_SESSION_ID" ] && PROVIDER_SESSION_ID=$(json_string_value conversationID || true)
[ -z "$PROVIDER_SESSION_ID" ] && PROVIDER_SESSION_ID=$(json_string_value conversationId || true)

TRANSCRIPT_PATH=$(json_string_value transcript_path || true)
[ -z "$TRANSCRIPT_PATH" ] && TRANSCRIPT_PATH=$(json_string_value transcriptPath || true)
[ -z "$TRANSCRIPT_PATH" ] && TRANSCRIPT_PATH=$(json_string_value provider_transcript_path || true)
[ -z "$TRANSCRIPT_PATH" ] && TRANSCRIPT_PATH=$(json_string_value providerTranscriptPath || true)

AGENT_ROLE=$(json_string_value agent_role || true)
[ -z "$AGENT_ROLE" ] && AGENT_ROLE=$(json_string_value unpeel_agent_role || true)
AGENT_KIND=$(json_string_value unpeel_agent_kind || true)
[ -z "$AGENT_KIND" ] && AGENT_KIND=$(json_string_value agent_kind || true)

build_projected_payload() {
  _p=$(printf '{"hook_event_name":"%s"' "$(json_escape_string "$EVENT_TYPE")")
  [ -n "$LAST_TOOL_NAME" ] && _p="$_p$(printf ',"tool_name":"%s"' "$(json_escape_string "$LAST_TOOL_NAME")")"
  [ -n "$UNPEEL_RUNTIME_ID" ] && _p="$_p$(printf ',"unpeel_runtime_id":"%s"' "$(json_escape_string "$UNPEEL_RUNTIME_ID")")"
  [ -n "$PROVIDER_SESSION_ID" ] && _p="$_p$(printf ',"session_id":"%s"' "$(json_escape_string "$PROVIDER_SESSION_ID")")"
  [ -n "$TRANSCRIPT_PATH" ] && _p="$_p$(printf ',"transcript_path":"%s"' "$(json_escape_string "$TRANSCRIPT_PATH")")"
  [ -n "$AGENT_ROLE" ] && _p="$_p$(printf ',"agent_role":"%s"' "$(json_escape_string "$AGENT_ROLE")")"
  [ -n "$AGENT_KIND" ] && _p="$_p$(printf ',"unpeel_agent_kind":"%s"' "$(json_escape_string "$AGENT_KIND")")"
  _gen="$(runtime_generation_json_field)"
  [ -n "$_gen" ] && _p="$_p$_gen"
  _ep="$(task_episode_json_field)"
  [ -n "$_ep" ] && _p="$_p$_ep"
  _p="$_p}"
  printf '%s' "$_p"
}

HOOK_PAYLOAD="$(build_projected_payload)"

if [ -n "$UNPEEL_SESSION_ID" ]; then
  HOOK_INGRESS_ACCEPTED=0
  HOOK_INGRESS_REJECTED=0
  post_to_unpeel() {
    # Several Unpeel instances can run at once (e.g. a dev build next to the
    # installed app) and they share the port registry. Post to every known
    # port, not just the first that answers, so the instance that owns this
    # session always receives the event.
    post_hook_payload "$HOOK_PAYLOAD" "$UNPEEL_SESSION_ID" "$UNPEEL_APP_PORT"
    post_hook_payload_to_current_ports "$HOOK_PAYLOAD" "$UNPEEL_SESSION_ID" "$UNPEEL_APP_PORT"
  }
  # Preflight known listeners in both sync and async before publishing state or seed.
  preflight_hook_payload_to_known_ports "$HOOK_PAYLOAD" "$UNPEEL_SESSION_ID"
  if [ "${HOOK_INGRESS_REJECTED:-0}" != "1" ]; then
    record_last_hook_event "$EVENT_TYPE" "$LAST_TOOL_NAME"
    if [ "${UNPEEL_HOOK_POST_SYNC:-}" = "1" ]; then
      post_to_unpeel
    else
      ( post_to_unpeel ) &
      exit 0
    fi
  fi
else
  record_last_hook_event "$EVENT_TYPE" "$LAST_TOOL_NAME"
fi

exit 0
"#;
