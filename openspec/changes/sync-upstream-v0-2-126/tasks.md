## 1. Preparation

- [x] 1.1 Confirm clean branch, merge base `c526d6f7`, 74 incoming commits and target `d5a01522`; read contracts and the adjustments file.
- [x] 1.2 Run `cargo fmt --all` on our side before the merge; commit only if it changes something.

## 2. Merge and resolution

- [x] 2.1 Start the real `--no-ff --no-commit` merge with zdiff3/rerere.
- [x] 2.2 D1: Cloud dormant; provider switching, provider rules, iPhone model picker and `quiesce_refresh` live; edge deploy/wrangler stay fork.
- [x] 2.3 D2: IPC bearer distributed to every fork client; background engine refused.
- [x] 2.4 D3: Muse arms in every fork match; Muse holds the execution lease; no MCP grants.
- [x] 2.5 D4: unified update plan with fork deltas; Pi `update --all`; compatibility feed on fork release base with 404 as empty.
- [x] 2.6 D5: native subagents switch only through the `sessions` grant (`SessionsGrant.side_chats_only`), `RunRequest.mcp` empty.
- [x] 2.7 D6: `Some([])` only for questions closed without an answer; update `crates/doc/AGENTS.md`.
- [x] 2.8 D7: terminal latency/link crates; Workers terminal ported; scroll/alternate screen/panel fork pieces preserved.
- [x] 2.9 Refused items resolved to the fork side (bumps, `e680d5c98`, `4879a4754`, media, upstream CI workflows, `.gitignore`).
- [x] 2.10 Android/landing inert, outside `default-members`.
- [x] 2.11 Remaining TAKE/ADAPT ports (rail, typed path, PR destination, session card, captions/SentMessage, new session, outbox, tween, Codex approval, provider hardening, subagent gates).
- [x] 2.12 iOS generated bindings regenerated or upstream copy kept with Mac handoff.
- [x] 2.13 `cargo fmt --all --check` and `cargo check --workspace --all-targets` green; commit merge without markers.

## 3. Verification

- [x] 3.1 `scripts/ci/test-core.sh` with new upstream tests listed.
- [x] 3.2 Focused `cargo test -p` for touched crates and `zeron-ui` lib tests.
- [x] 3.3 Edge typecheck and tests.
- [x] 3.4 Baseline proof for any failure claimed as preexisting (doc_host `legacy_admission_rechecks_owner_and_retires_only_the_captured_wake` fails identically on 0aa2344b).
- [x] 3.5 Line-by-line audit of accepted commits' additions against the result.

## 4. Closeout

- [x] 4.1 `.upstream-sync/history/<UTC>-d5a01522.yaml` with `outcome: prepared`.
- [x] 4.2 `docs/upstream-sync.md` tables and fork version (1.0.10).
- [x] 4.3 Root `AGENTS.md` (terminal RTC, new crates) and affected crate `AGENTS.md`.
- [x] 4.4 Report in `.tmp/reports/sync-upstream-20261026.md`.
- [ ] 4.5 Native macOS/iOS acceptance (handoff; unavailable on Linux).

## 5. Owner follow-ups

- [ ] 5.1 Decide whether Workers-started servers should regain previews/Ports (no `ZERON_ORIGIN` stamp from workers-unpeel).
- [ ] 5.2 Archive this change after native acceptance.
