#!/usr/bin/env bash
#
# Headless end-to-end verification of the native terminal pipeline:
#
#   unpeel-host (PTY + output.bin + session.sock)
#     → unpeel-attach (replay + kqueue live follow + stdin relay)
#       → ht (libghostty-vt screen, montanaflynn/headless-terminal)
#
# ht renders unpeel-attach through the same VT engine a Ghostty surface
# uses, but with no Metal/GUI — so this runs where ghostty_surface_new
# cannot initialize (CI, agent sandboxes; see memory note from 2026-06-12).
#
# Verifies:
#   1. replay   — output produced BEFORE attach appears on the screen
#   2. echo     — keys typed through the attach client round-trip
#                 (ht → attach stdin → control socket → host PTY →
#                  output.bin → kqueue follow → attach stdout → VT)
#   3. reattach — a second attach replays content from the first epoch
#
# Usage: apps/native/verify-attach.sh
# Exit:  0 = all checks pass; non-zero with a FAIL line otherwise.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
ATTACH_BIN="$REPO_ROOT/apps/native/unpeel-attach/target/debug/unpeel-attach"
HOST_BIN="$REPO_ROOT/crates/target/debug/unpeel-host"
HT="$(command -v ht || true)"
[ -n "$HT" ] || HT="$REPO_ROOT/apps/native/tools/ht"

# Short id: the session dir name feeds a unix socket path (SUN_LEN ≤ ~104).
SID="vfy-$(uuidgen | cut -c1-8 | tr '[:upper:]' '[:lower:]')"
SESSION_DIR="$HOME/.unpeel/app-sessions/$SID"
LAUNCH_FILE="$(mktemp -t unpeel-verify-launch)"
HT_NAME_1="unpeel-verify-1-$$"
HT_NAME_2="unpeel-verify-2-$$"

cleanup() {
  "$HT" kill "$HT_NAME_1" >/dev/null 2>&1 || true
  "$HT" kill "$HT_NAME_2" >/dev/null 2>&1 || true
  "$HT" remove "$HT_NAME_1" >/dev/null 2>&1 || true
  "$HT" remove "$HT_NAME_2" >/dev/null 2>&1 || true
  if [ -S "$SESSION_DIR/session.sock" ]; then
    printf '{"type":"kill"}\n' | nc -U -w 2 "$SESSION_DIR/session.sock" >/dev/null 2>&1 || true
    sleep 0.5
  fi
  rm -rf "$SESSION_DIR" "$LAUNCH_FILE"
}
trap cleanup EXIT

fail() {
  echo "FAIL: $*" >&2
  echo "--- last screen ($HT_NAME_1):" >&2
  "$HT" view "$HT_NAME_1" 2>/dev/null | tail -8 >&2 || true
  exit 1
}

step() { echo "==> $*"; }

# --- 0. Prerequisites -------------------------------------------------------

[ -x "$HT" ] || fail "ht not found; install with: cd apps/native/tools && \
curl -sL https://github.com/montanaflynn/headless-terminal/releases/download/v0.3.0/ht-v0.3.0-darwin-arm64.tar.gz | tar xz"

if [ ! -x "$ATTACH_BIN" ]; then
  step "building unpeel-attach"
  (cd "$REPO_ROOT/apps/native/unpeel-attach" && cargo build --quiet)
fi
if [ ! -x "$HOST_BIN" ]; then
  step "building unpeel-host"
  (cd "$REPO_ROOT/crates" && cargo build --quiet --bin unpeel-host)
fi

# --- 1. Start a hosted session ----------------------------------------------

step "starting hosted session $SID"
cat > "$LAUNCH_FILE" <<EOF
{
  "session": {
    "id": "$SID",
    "project_id": "verify-attach",
    "label": "verify attach",
    "custom_title": false,
    "command": "",
    "created_at": $(date +%s)000,
    "tag_id": null,
    "worktree_path": null,
    "worktree_branch": null
  },
  "cwd": "/tmp",
  "dark_mode": true,
  "hook_port": null,
  "initial_cols": 100,
  "initial_rows": 30
}
EOF
"$HOST_BIN" "$LAUNCH_FILE" &

for _ in $(seq 1 40); do
  [ -S "$SESSION_DIR/session.sock" ] && break
  sleep 0.1
done
[ -S "$SESSION_DIR/session.sock" ] || fail "session host never created its control socket"

# --- 2. Replay: produce output BEFORE attaching ------------------------------

# The typed command renders as 'REPLAY_$((40+2))' on screen, so the marker
# 'REPLAY_42' can only come from real shell OUTPUT — no false match on the
# echoed command line.
#
# Wait for the shell to draw its prompt (output.bin non-empty) before typing:
# keystrokes that land while zsh is still initializing zle can be discarded,
# which used to make this step flaky under load.
step "seeding pre-attach output"
for _ in $(seq 1 50); do
  [ -s "$SESSION_DIR/output.bin" ] && break
  sleep 0.1
done
[ -s "$SESSION_DIR/output.bin" ] || fail "shell never produced a prompt"
printf '{"type":"write","data":"echo REPLAY_$((40+2))\\r"}\n' \
  | nc -U -w 2 "$SESSION_DIR/session.sock" >/dev/null \
  || fail "could not write to the session control socket"
sleep 1

step "attaching (ht session $HT_NAME_1)"
"$HT" run --name "$HT_NAME_1" --size 100x30 "$ATTACH_BIN" "$SID" >/dev/null
"$HT" wait "$HT_NAME_1" --text "REPLAY_42" \
  || fail "replayed output never appeared on the attached screen"
echo "    replay OK"

# --- 3. Live echo: type through the attach client ----------------------------

step "typing through the attach client"
"$HT" send "$HT_NAME_1" 'echo LIVE_$((50+5))<CR>' >/dev/null
"$HT" wait "$HT_NAME_1" --text "LIVE_55" \
  || fail "live round-trip output never appeared (input relay or kqueue follow broken)"
echo "    live echo OK"

# --- 4. Reattach: a fresh attach replays history ------------------------------

step "detaching and reattaching (ht session $HT_NAME_2)"
"$HT" kill "$HT_NAME_1" >/dev/null
"$HT" run --name "$HT_NAME_2" --size 100x30 "$ATTACH_BIN" "$SID" >/dev/null
"$HT" wait "$HT_NAME_2" --text "LIVE_55" \
  || { HT_NAME_1="$HT_NAME_2"; fail "reattach replay missing first-epoch output"; }
echo "    reattach replay OK"

echo "PASS: replay, live echo, and reattach all verified"
