# Tasks: Quiet Runtime Log Defects

## 1. Diagnosis

- [x] 1.1 Trace each log signature to code (cursor repair, usage backoff, peer dial loop, quota, discovery, quit, IPC probe, command payload)

## 2. Engine / sync

- [x] 2.1 Live cursor certification in `ChatPersistence`; regression `first_verified_write_certifies_the_warm_handle`
- [x] 2.2 Escalating usage backoff + episode logging; regressions in `agent_accounts.rs`
- [x] 2.3 Readable model discovery reason; deferred case at debug
- [x] 2.4 Quota episode logging in `chat_client.rs`

## 3. RPC / harness / preview / doc

- [x] 3.1 `host_offline` offline cooldown in `LinkCache`; regression `host_offline_cooldown_survives_online_broadcasts`; existing assertions updated
- [x] 3.2 Rank `cmux-cli-shims` below stable installs; regression in `executable.rs`
- [x] 3.3 Preview scanner skips Zeron engines; regression in `service.rs`
- [x] 3.4 Attach command entries whole; classify in-flight reads

## 4. UI / edge

- [x] 4.1 Drain in-process engine before GPUI quit
- [x] 4.2 Edge quota counts admitted pushes only; resized limits; `npm -C edge run typecheck && npm -C edge run test`

## 5. Closeout

- [x] 5.1 DOX pass on owning `AGENTS.md` files
- [x] 5.2 Focused Rust tests via `scripts/cargo-verify.py` (2026-10-01: `zeron-ui` check; doc 170, engine 92 filtered, sync chat_client 36, rpc incl. `device_room`, harness `executable` 18 in foreground — green. Preview: scanner test green; 4 WebRTC/ICE tests fail on this host's network, unrelated)
- [ ] 5.3 Headed `cargo run` quit smoke (no `app_will_quit` timeout)
- [ ] 5.4 Publish edge (push touching `edge/` deploys) — user decision
