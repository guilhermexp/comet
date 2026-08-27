#!/usr/bin/env bash
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$HERE/../.." && pwd)"
bun "$REPO_ROOT/scripts/generate-runtime-client-catalog.mjs" --check
"$HERE/build-rust-bridge.sh" debug
cd "$HERE/UnpeelNative"
swift test "$@"
