#!/usr/bin/env bash
# The Linux Rust tests that used to run as separate `cargo test` invocations in
# session-sync-regressions, cursor-compatibility and preview-tests, as ONE
# build and ONE nextest run (separate invocations recompiled the same
# dependency graph with different feature sets).
#
# Same coverage as the old invocations:
#   zeron-harness  every test binary (cursor-compatibility ran `-p zeron-harness`
#                  plus `--test pi_rpc` with native-fixture; native-fixture only
#                  adds the pi_rpc fixture binaries, so it is on for the whole build)
#   zeron-preview  every test binary (preview-tests)
#   zeron-doc      lib + persistence and memory regressions + attachments_roundtrip
#   zeron-sync, zeron-update   lib only (session-sync-regressions)
#   zeron-engine   lib + the integration tests named below. Engine has many other
#                  integration binaries (live agents etc.) that no workflow ran, so
#                  they are listed instead of globbed to avoid compiling them.
#   zeron-orb     lib + geometry (the former UI orb geometry tests)
# Voice media/session, RPC and MCP keep their dedicated voice-tests.yml coverage.
# nextest does not run doctests; these crates have none (their doc comments
# contain no Rust code blocks).
# Native visual model/GPU and zatex unit suites are included as workspace capabilities.
# Extra arguments are passed to nextest (the nightly job adds `--release`).
# NEXTEST_PROFILE=ci (pull requests and main) leaves out the slow and soak tests
# that scripts/ci/nextest.toml lists; the nightly runs use the default profile.
set -euo pipefail
cd "$(dirname "$0")/../.."

# Test binaries that run nothing on Linux are left out: they would still be
# compiled and linked. Windows-only files (#![cfg(windows)]), and files whose
# every test is #[ignore]d because it needs a live service or a real CLI.
skip=" codex_availability windows_native managed_install muse_account_live pi_live real_acp_lifecycle real_quiet_survey coordinator "
tests=()
for f in crates/harness/tests/*.rs crates/preview/tests/*.rs crates/doc/tests/*.rs; do
  name="$(basename "$f" .rs)"
  [[ "$skip" == *" $name "* ]] || tests+=(--test "$name")
done
for t in misc geometry session_publication restart_resume side_chats provider_switch run_recovery message_queue; do
  tests+=(--test "$t")
done

exec cargo nextest run --config-file scripts/ci/nextest.toml --locked --no-fail-fast \
  -p zeron-harness -p zeron-engine -p zeron-sync -p zeron-update -p zeron-doc -p zeron-preview \
  -p zeron-visual -p zeron-visual-gpu -p zatex -p zeron-orb \
  --features zeron-harness/native-fixture,zatex/bundled \
  --lib --bins "${tests[@]}" "$@"
