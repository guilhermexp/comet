#!/bin/sh
umask 077
INPUT=$(cat)

# Muse runs plugin hooks with a scrubbed environment (verified 0.1.0: only
# MUSE_PLUGIN_*/PLUGIN_* survive), so the Unpeel identity exported into the
# session PTY never reaches this process directly. The parent of this hook IS
# the muse process, which does carry that environment — recover it from there
# (`ps eww` shows a same-user process's env). Values are UUID/port/path
# shaped; the path patterns stop at whitespace, which every Unpeel-managed
# home satisfies.
recover_parent_env() {
  ps eww $PPID 2>/dev/null | grep -oE "$1=$2" | head -1 | cut -d= -f2-
}
[ -n "${UNPEEL_SESSION_ID:-}" ] \
  || UNPEEL_SESSION_ID="$(recover_parent_env UNPEEL_SESSION_ID '[0-9a-fA-F-]+')"
[ -n "${UNPEEL_APP_PORT:-}" ] \
  || UNPEEL_APP_PORT="$(recover_parent_env UNPEEL_APP_PORT '[0-9]+')"
[ -n "${UNPEEL_SESSION_DIR:-}" ] \
  || UNPEEL_SESSION_DIR="$(recover_parent_env UNPEEL_SESSION_DIR '[^[:space:]]+')"
[ -n "${UNPEEL_APP_PORT_REGISTRY_FILE:-}" ] \
  || UNPEEL_APP_PORT_REGISTRY_FILE="$(recover_parent_env UNPEEL_APP_PORT_REGISTRY_FILE '[^[:space:]]+')"
[ -n "${UNPEEL_HOOK_TRACE_FILE:-}" ] \
  || UNPEEL_HOOK_TRACE_FILE="$(recover_parent_env UNPEEL_HOOK_TRACE_FILE '[^[:space:]]+')"
[ -n "${UNPEEL_RUNTIME_GENERATION:-}" ] \
  || UNPEEL_RUNTIME_GENERATION="$(recover_parent_env UNPEEL_RUNTIME_GENERATION '[0-9]+')"
[ -n "${UNPEEL_HOOK_POST_SYNC:-}" ] \
  || UNPEEL_HOOK_POST_SYNC="$(recover_parent_env UNPEEL_HOOK_POST_SYNC '[01]')"

TRACE_FILE="${UNPEEL_HOOK_TRACE_FILE:-$HOME/.zeron/workers/hooks/trace.log}"
mkdir -p "$(dirname "$TRACE_FILE")" >/dev/null 2>&1 || true
if [ -f "$TRACE_FILE" ]; then
  _unpeel_trace_size=$(wc -c < "$TRACE_FILE" 2>/dev/null | tr -d ' ')
  if [ -n "$_unpeel_trace_size" ] && [ "$_unpeel_trace_size" -gt 10485760 ]; then
    mv -f "$TRACE_FILE" "$TRACE_FILE.1" 2>/dev/null || true
  fi
fi
# The payload contains the user's prompt text; only log it when explicitly
# opted in. Otherwise record that an event fired without its contents.
if [ "${UNPEEL_HOOK_TRACE_VERBOSE:-}" = "1" ]; then
  TRACE_PAYLOAD="$INPUT"
else
  TRACE_PAYLOAD="<redacted; UNPEEL_HOOK_TRACE_VERBOSE=1 to log>"
fi
UNPEEL_PORT_REGISTRY_FILE="${UNPEEL_APP_PORT_REGISTRY_FILE:-$HOME/.unpeel/app-ports}"

json_escape_string() {
  printf '%s' "$1" | sed 's/\\/\\\\/g; s/"/\\"/g'
}

json_string_value() {
  _key="$1"
  printf '%s' "$INPUT" | grep -oE "\"$_key\"[[:space:]]*:[[:space:]]*\"[^\"]*\"" | head -1 | sed 's/.*: *"\([^"]*\)".*/\1/'
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
  _hook_status=$(printf '%s\n' "$_hook_response" | tail -n 1)
  _hook_body=$(printf '%s\n' "$_hook_response" | sed '$d')
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
  _record_payload="$_record_payload,\"unpeel_runtime_id\":\"muse\""
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
  _record_payload="$_record_payload}"

  _record_tmp="$_record_dir/.last-hook-event.json.$$"
  if printf '%s' "$_record_payload" > "$_record_tmp" 2>/dev/null; then
    mv -f "$_record_tmp" "$_record_dir/last-hook-event.json" 2>/dev/null \
      || rm -f "$_record_tmp" 2>/dev/null || true
  fi
}

printf '%s muse-hook session=%s port=%s payload=%s\n' \
  "$(date '+%Y-%m-%d %H:%M:%S')" \
  "${UNPEEL_SESSION_ID:-}" \
  "${UNPEEL_APP_PORT:-}" \
  "$TRACE_PAYLOAD" >> "$TRACE_FILE" 2>/dev/null || true

LAST_EVENT_NAME=$(printf '%s' "$INPUT" | grep -oE '"hook_event_name"[[:space:]]*:[[:space:]]*"[^"]*"' | grep -oE '"[^"]*"$' | tr -d '"')
LAST_TOOL_NAME=$(printf '%s' "$INPUT" | grep -oE '"tool_name"[[:space:]]*:[[:space:]]*"[^"]*"' | grep -oE '"[^"]*"$' | tr -d '"')

# Muse fires PermissionRequest for its internal reminder-decision tool; the
# decision is answered inside muse without any user-facing prompt and can
# arrive after Stop, so forwarding it would latch attention with nothing
# left to clear it.
if [ "$LAST_EVENT_NAME" = "PermissionRequest" ] \
  && [ "$LAST_TOOL_NAME" = "submit_reminder_decision" ]; then
  exit 0
fi

PROVIDER_SESSION_ID="$(json_string_value session_id || true)"
[ -z "$PROVIDER_SESSION_ID" ] && PROVIDER_SESSION_ID="$(json_string_value provider_session_id || true)"
[ -z "$PROVIDER_SESSION_ID" ] && PROVIDER_SESSION_ID="$(json_string_value sessionId || true)"

TRANSCRIPT_PATH="$(json_string_value transcript_path || true)"
[ -z "$TRANSCRIPT_PATH" ] && TRANSCRIPT_PATH="$(json_string_value transcriptPath || true)"

build_projected_payload() {
  _p=$(printf '{"hook_event_name":"%s"' "$(json_escape_string "$LAST_EVENT_NAME")")
  [ -n "$LAST_TOOL_NAME" ] && _p="$_p$(printf ',"tool_name":"%s"' "$(json_escape_string "$LAST_TOOL_NAME")")"
  _p="$_p,\"unpeel_runtime_id\":\"muse\""
  [ -n "$PROVIDER_SESSION_ID" ] && _p="$_p$(printf ',"session_id":"%s"' "$(json_escape_string "$PROVIDER_SESSION_ID")")"
  [ -n "$TRANSCRIPT_PATH" ] && _p="$_p$(printf ',"transcript_path":"%s"' "$(json_escape_string "$TRANSCRIPT_PATH")")"
  _gen="$(runtime_generation_json_field)"
  [ -n "$_gen" ] && _p="$_p$_gen"
  _ep="$(task_episode_json_field)"
  [ -n "$_ep" ] && _p="$_p$_ep"
  _p="$_p}"
  printf '%s' "$_p"
}

HOOK_PAYLOAD="$(build_projected_payload)"

if [ -n "${UNPEEL_SESSION_ID:-}" ]; then
  HOOK_INGRESS_ACCEPTED=0
  HOOK_INGRESS_REJECTED=0
  post_to_unpeel() {
    post_hook_payload "$HOOK_PAYLOAD" "$UNPEEL_SESSION_ID" "${UNPEEL_APP_PORT:-}"
    post_hook_payload_to_current_ports "$HOOK_PAYLOAD" "$UNPEEL_SESSION_ID" "${UNPEEL_APP_PORT:-}"
  }
  preflight_hook_payload_to_known_ports "$HOOK_PAYLOAD" "$UNPEEL_SESSION_ID"
  if [ "${HOOK_INGRESS_REJECTED:-0}" != "1" ]; then
    record_last_hook_event "$LAST_EVENT_NAME" "$LAST_TOOL_NAME"
    if [ "${UNPEEL_HOOK_POST_SYNC:-}" = "1" ]; then
      post_to_unpeel
    else
      ( post_to_unpeel ) &
      exit 0
    fi
  fi
else
  record_last_hook_event "$LAST_EVENT_NAME" "$LAST_TOOL_NAME"
fi

exit 0
