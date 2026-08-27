#!/usr/bin/env bash
#
# Headless end-to-end verification of the Browser MCP pipeline:
#
#   unpeel-host __mcp__ `browser` tool (tool → argv/env translation, grants)
#     → agent-browser native daemon (CDP)
#       → system Chrome/Chromium
#
# The engine's `--native` mode is experimental upstream, and integration
# already survived three engine landmines (profile+allowlist daemon wedge,
# --use-mock-keychain cookie purges, STREAM_PORT no-op) — run this after every
# agent-browser version bump BEFORE raising the pinned version, and after
# changes to browser_mcp.rs. See docs/feature/browser-mcp-deep-check.md
# ("Native-Mode Findings") for why each check exists.
#
# Verifies:
#   1. engine    — binary resolves; open/snapshot-refs/screenshot/close work
#   2. rules     — ALLOWED_DOMAINS blocks an off-list navigation
#   3. logins    — SESSION_NAME + ENCRYPTION_KEY persist a cookie across a
#                  full browser restart, state file is encrypted (.json.enc)
#   4. keychain  — probes whether profile-dir cookies still purge on restart
#                  (informational: flips when upstream drops --use-mock-keychain)
#   5. mcp       — the unified host's `browser` tool end to end: grant gate
#                  refuses when off, open + screenshot produce a session artifact
#
# Usage: apps/native/verify-browser.sh
# Exit:  0 = all checks pass; non-zero with a FAIL line otherwise.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
HOST_BIN="$REPO_ROOT/crates/target/debug/unpeel-host"

step() { printf '\n==> %s\n' "$1"; }
pass() { printf 'PASS: %s\n' "$1"; }
fail() { printf 'FAIL: %s\n' "$1" >&2; exit 1; }

# --- engine resolution (same order as browser_mcp.rs / build-app.sh) --------
ENGINE="${UNPEEL_BROWSER_BIN:-}"
if [ -z "$ENGINE" ]; then
  for candidate in \
    "$HOME/.unpeel/browser/bin/agent-browser" \
    "$(command -v agent-browser 2>/dev/null || true)"; do
    [ -n "$candidate" ] && [ -e "$candidate" ] || continue
    resolved="$(readlink -f "$candidate" 2>/dev/null || echo "$candidate")"
    if head -c 2 "$resolved" 2>/dev/null | grep -q '#!'; then
      arch="$(uname -m | sed 's/x86_64/x64/')"
      native="$(dirname "$resolved")/agent-browser-darwin-$arch"
      [ -f "$native" ] && resolved="$native" || continue
    fi
    [ -f "$resolved" ] && { ENGINE="$resolved"; break; }
  done
fi
[ -n "$ENGINE" ] && [ -f "$ENGINE" ] || fail "agent-browser engine not found"
pass "engine resolved: $ENGINE"

step "building unpeel-host (debug)"
(cd "$REPO_ROOT/crates" && cargo build -q -p unpeel-host)

SCRATCH="$(mktemp -d)"
ENGINE_HOME="$(mktemp -d)"
cleanup() {
  for sess in vb-core vb-rules vb-state vb-keychain; do
    env -i HOME="$ENGINE_HOME" PATH=/usr/bin:/bin \
      AGENT_BROWSER_SESSION="$sess" AGENT_BROWSER_NATIVE=1 \
      "$ENGINE" close >/dev/null 2>&1 || true
  done
  UNPEEL_HOME="$SCRATCH" UNPEEL_BROWSER_BIN="$ENGINE" \
    "$HOST_BIN" __browser_cleanup__ vb-session >/dev/null 2>&1 || true
  # The MCP check's engine daemon runs with the real HOME, so its login-state
  # file lands in the real ~/.agent-browser — remove the test project's.
  rm -f "$HOME/.agent-browser/sessions/unpeel-proj-vb-proj-"* 2>/dev/null || true
  rm -rf "$SCRATCH" "$ENGINE_HOME"
}
trap cleanup EXIT

engine() { # engine <session> [VAR=value ...] <engine subcommand + args...>
  local sess="$1"; shift
  local envs=()
  while [ $# -gt 0 ] && [[ "$1" == *=* ]]; do envs+=("$1"); shift; done
  env -i HOME="$ENGINE_HOME" PATH=/usr/bin:/bin \
    AGENT_BROWSER_SESSION="$sess" AGENT_BROWSER_NATIVE=1 \
    AGENT_BROWSER_SOCKET_DIR="$ENGINE_HOME/sockets" \
    ${envs[@]+"${envs[@]}"} "$ENGINE" "$@"
}

# --- 1. core engine loop ------------------------------------------------------
step "engine core loop"
out="$(engine vb-core open https://example.com 2>&1)" \
  || fail "open example.com: $out"
snap="$(engine vb-core snapshot -i 2>&1)"
echo "$snap" | grep -q "ref=" || fail "snapshot has no refs: $snap"
shot="$ENGINE_HOME/shot.png"
engine vb-core screenshot "$shot" >/dev/null 2>&1
[ -s "$shot" ] || fail "screenshot not written"
engine vb-core close >/dev/null 2>&1
pass "open + snapshot refs + screenshot + close"

# --- 2. site rules enforced ---------------------------------------------------
step "site rules"
blocked="$(engine vb-rules AGENT_BROWSER_ALLOWED_DOMAINS=example.com \
  open https://vercel.com 2>&1 || true)"
echo "$blocked" | grep -qi "not in the allowed domains" \
  || fail "allowlist did not block off-list domain: $blocked"
engine vb-rules close >/dev/null 2>&1 || true
pass "ALLOWED_DOMAINS blocks navigation"

# --- 3. login persistence (state save/restore + encryption) -------------------
step "login persistence"
KEY="0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
state() { engine vb-state AGENT_BROWSER_SESSION_NAME=vb-proj \
  AGENT_BROWSER_ENCRYPTION_KEY="$KEY" "$@"; }
state open https://example.com >/dev/null 2>&1
state eval "document.cookie='vb_login=alive; max-age=86400; path=/'" >/dev/null 2>&1
state close >/dev/null 2>&1; sleep 1
ls "$ENGINE_HOME/.agent-browser/sessions/"vb-proj-*.json.enc >/dev/null 2>&1 \
  || fail "encrypted state file (.json.enc) not written"
state open https://example.com >/dev/null 2>&1
restored="$(state eval "document.cookie" 2>/dev/null | head -1)"
state close >/dev/null 2>&1
echo "$restored" | grep -q "vb_login=alive" \
  || fail "cookie did not survive browser restart via state restore: $restored"
pass "cookie survives restart; state encrypted at rest"

# --- 4. mock-keychain probe (informational) -----------------------------------
step "mock-keychain probe"
KPROF="$ENGINE_HOME/kprof"
kc() { engine vb-keychain AGENT_BROWSER_PROFILE="$KPROF" "$@"; }
kc open https://example.com >/dev/null 2>&1
kc eval "document.cookie='vb_keychain=probe; max-age=86400; path=/'" >/dev/null 2>&1
kc close >/dev/null 2>&1; sleep 1
kc open https://example.com >/dev/null 2>&1
kprobe="$(kc eval "document.cookie" 2>/dev/null | head -1)"
kc close >/dev/null 2>&1
if echo "$kprobe" | grep -q "vb_keychain=probe"; then
  echo "NOTE: profile-dir cookies now SURVIVE restarts — upstream may have"
  echo "      dropped --use-mock-keychain. Revisit the SESSION_NAME workaround"
  echo "      and the seed-from-chrome removal (deep-check doc)."
else
  echo "note: profile-dir cookies still purge on restart (known: --use-mock-keychain)"
fi

# --- 5. MCP server end to end --------------------------------------------------
step "mcp server"
mkdir -p "$SCRATCH/app-sessions/vb-session"
cat > "$SCRATCH/app-sessions/vb-session/manifest.json" <<'EOF'
{"session":{"id":"vb-session","project_id":"vb-proj","label":"t","custom_title":false,"command":"claude","created_at":1},"cwd":"/tmp","state":"running","pid":1,"exit_code":null,"heartbeat_at":1,"updated_at":1,"browser_mcp_enabled":true}
EOF
cat > "$SCRATCH/app-state.json" <<'EOF'
{"projects":[{"id":"vb-proj","name":"VB","path":"/tmp"}],"presets":[],"active_tabs":{},"browser_access":{"vb-session":"off"}}
EOF
mcp() { UNPEEL_HOME="$SCRATCH" UNPEEL_SESSION_ID=vb-session \
  UNPEEL_BROWSER_BIN="$ENGINE" "$HOST_BIN" __mcp__; }

refused="$(printf '%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"browser","arguments":{"action":"open","url":"https://example.com"}}}' \
  | mcp)"
echo "$refused" | grep -q '"isError":true' && echo "$refused" | grep -qi "browser access" \
  || fail "grant-off call was not refused: $refused"
pass "grant gate refuses when access is off"

python3 - "$SCRATCH/app-state.json" <<'EOF'
import json, sys
p = sys.argv[1]
d = json.load(open(p)); d["browser_access"] = {"vb-session": "on"}; json.dump(d, open(p, "w"))
EOF
result="$(printf '%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"browser","arguments":{"action":"open","url":"https://example.com"}}}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"browser","arguments":{"action":"screenshot"}}}' \
  '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"browser","arguments":{"action":"close"}}}' \
  | mcp)"
echo "$result" | grep -q "Saved screenshot to" || fail "mcp screenshot failed: $result"
ls "$SCRATCH/app-sessions/vb-session/artifacts/browser/screenshots/"*.png >/dev/null 2>&1 \
  || fail "screenshot artifact missing from session dir"
pass "browser open + screenshot produce a session artifact"

printf '\nAll browser MCP checks passed.\n'
