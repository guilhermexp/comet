#!/usr/bin/env bash
# End-to-end tests for the terminal UI and CLI.
#
#   ./run.sh                 # every case
#   ./run.sh archive unread  # only cases whose name contains one of these
#   ./run.sh -v archive      # stream the case's own output too
#
# Each case gets a freshly built UNPEEL_HOME and runs in its own process, so
# one case can never leave state that changes another's result. Cases are
# sequential on purpose: they bind ports, spawn hosted processes, and drive a
# PTY, none of which parallelises safely.
set -uo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
crates="$(cd "$here/../.." && pwd)"

verbose=0
filters=()
for arg in "$@"; do
  case "$arg" in
    -v|--verbose) verbose=1 ;;
    *) filters+=("$arg") ;;
  esac
done

binary="${UNPEEL_TUI_BINARY:-$crates/target/debug/unpeel}"
if [[ -z "${UNPEEL_TUI_SKIP_BUILD:-}" ]]; then
  echo "building unpeel + unpeel-host…"
  # Real lifecycle cases launch the sibling host binary. Rebuilding only the
  # TUI can silently test fresh client code against a stale Host executable.
  if ! (cd "$crates" && cargo build -p unpeel-tui -p unpeel-host 2>&1 | tail -3); then
    echo "build failed" >&2
    exit 1
  fi
fi
if [[ ! -x "$binary" ]]; then
  echo "no binary at $binary" >&2
  exit 1
fi
export UNPEEL_TUI_BINARY="$binary"

# A UNIX socket path is capped near 104 bytes (sockaddr_un) and hosted
# sessions bind <home>/app-sessions/<uuid>/session.sock — about 55 bytes of
# suffix. macOS TMPDIR alone is ~49, which blows the limit and makes hosts
# fail to bind with no visible error: sessions simply never start. Use /tmp
# unless the caller names something short themselves.
base="${UNPEEL_TUI_TEST_BASE:-/tmp/ut}"

pass=0; fail=0; failed_cases=()
start_all=$(date +%s)

for case_file in "$here"/cases/*.py; do
  name="$(basename "$case_file" .py)"
  if (( ${#filters[@]} )); then
    keep=0
    for filter in "${filters[@]}"; do
      [[ "$name" == *"$filter"* ]] && keep=1
    done
    (( keep )) || continue
  fi

  home="$base-$name"
  rm -rf "$home"
  printf '%-22s ' "$name"
  start=$(date +%s)
  output="$(UNPEEL_TUI_TEST_HOME="$home" python3 "$case_file" 2>&1)"
  status=$?
  elapsed=$(( $(date +%s) - start ))

  case_pass=$(grep -c '^PASS ' <<<"$output")
  case_fail=$(grep -c '^FAIL ' <<<"$output")
  pass=$(( pass + case_pass ))
  fail=$(( fail + case_fail ))

  if (( status == 0 )); then
    printf '\033[32mok\033[0m   %2d checks  %2ds\n' "$case_pass" "$elapsed"
  else
    printf '\033[31mFAIL\033[0m %2d/%d failed %2ds\n' "$case_fail" \
      "$(( case_pass + case_fail ))" "$elapsed"
    failed_cases+=("$name")
    grep '^FAIL ' <<<"$output" | sed 's/^/    /'
  fi
  if (( verbose )); then
    sed 's/^/    | /' <<<"$output"
  else
    grep '^NOTE ' <<<"$output" | sed 's/^/    /'
  fi

  # Sweep any hosted process this case left behind BEFORE deleting its home.
  # A leaked host keeps writing to an output.bin we just unlinked, so the
  # space is never reclaimed and the disk fills up over a long run — which
  # is exactly how a full suite once ran the machine out of space.
  ps -eo pid,command \
    | grep "__session_host__" \
    | grep -F "$home/" \
    | awk '{print $1}' \
    | xargs -r kill -9 2>/dev/null || true
  rm -rf "$home"
done

echo
printf 'total: %d passed, %d failed, %ds\n' "$pass" "$fail" \
  "$(( $(date +%s) - start_all ))"
if (( ${#failed_cases[@]} )); then
  printf 'failing cases: %s\n' "${failed_cases[*]}"
  exit 1
fi
exit 0
