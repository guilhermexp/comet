## 1. Preparation

- [ ] 1.1 Confirm clean branch, merge base `c526d6f7`, 74 incoming commits and target `d5a01522`; read contracts and the adjustments file.
- [ ] 1.2 Run `cargo fmt --all` on our side before the merge; commit only if it changes something.

## 2. Merge and resolution

- [ ] 2.1 Start the real `--no-ff --no-commit` merge with zdiff3/rerere.
- [ ] 2.2 D1: Cloud dormant; provider switching, provider rules, iPhone model picker and `quiesce_refresh` live; edge deploy/wrangler stay fork.
- [ ] 2.3 D2: IPC bearer distributed to every fork client; background engine refused.
- [ ] 2.4 D3: Muse arms in every fork match; Muse holds the execution lease; no MCP grants.
- [ ] 2.5 D4: unified update plan with fork deltas; Pi `update --all`; compatibility feed on fork release base with 404 as empty.
- [ ] 2.6 D5: native subagents switch only through the `sessions` grant, `RunRequest.mcp` empty.
- [ ] 2.7 D6: `Some([])` only for questions closed without an answer; update `crates/doc/AGENTS.md`.
- [ ] 2.8 D7: terminal latency/link crates; Workers terminal ported; scroll/alternate screen/panel fork pieces preserved.
- [ ] 2.9 Refused items resolved to the fork side (bumps, `e680d5c98`, `4879a4754`, media, upstream CI workflows, `.gitignore`).
- [ ] 2.10 Android/landing inert, outside `default-members`.
- [ ] 2.11 Remaining TAKE/ADAPT ports (rail, typed path, PR destination, session card, captions/SentMessage, new session, outbox, tween, Codex approval, provider hardening, subagent gates).
- [ ] 2.12 iOS generated bindings regenerated or upstream copy kept with Mac handoff.
- [ ] 2.13 `cargo fmt --all --check` and `cargo check --workspace --all-targets` green; commit merge without markers.

## 3. Verification

- [ ] 3.1 `scripts/ci/test-core.sh` with new upstream tests listed.
- [ ] 3.2 Focused `cargo test -p` for touched crates and `zeron-ui` lib tests.
- [ ] 3.3 Edge typecheck and tests.
- [ ] 3.4 Baseline proof for any failure claimed as preexisting.
- [ ] 3.5 Line-by-line audit of accepted commits' additions against the result.

## 4. Closeout

- [ ] 4.1 `.upstream-sync/history/<UTC>-d5a01522.yaml` with `outcome: prepared`.
- [ ] 4.2 `docs/upstream-sync.md` tables and fork version (1.0.10).
- [ ] 4.3 Root `AGENTS.md` (terminal RTC, new crates) and affected crate `AGENTS.md`.
- [ ] 4.4 Report in `.tmp/reports/sync-upstream-20261026.md`.
- [ ] 4.5 Native macOS/iOS acceptance (handoff; unavailable on Linux).
