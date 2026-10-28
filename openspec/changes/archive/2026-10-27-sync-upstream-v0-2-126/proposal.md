## Why

Integrate the 74 incoming upstream commits from `c526d6f7` through the pinned `d5a0152265112c235d79b29df92b550c73a5ca1a` (v0.2.126) into fork baseline `0aa2344b` (fork 1.0.10) by real merge. The owner asked to bring everything convenient and keep the fork's private changes. Local integration and proof only: no push, tags, PR or deploy.

## What Changes

- D1 (`cc7da5c2a` + satellites, ADAPT): Zeron Cloud enters dormant (runner, credentials, Cloud client/pages, `CheckoutKind::Cloud`, iOS `Cloud/*` inert without `ZERON_RUNNER_*` or an available Cloud status). Provider switching between turns, provider/account rules, iPhone model picker and `Auth::quiesce_refresh` enter live. Edge deploy/wrangler stay the fork's.
- D2 (`cc7da5c2a`, `acd7c9ba7`): IPC bearer (`ipc_auth`, `serve_ipc`, `connect_ws(url, token)`, `connect_ipc`) and large-message fragmentation accepted and distributed to every fork client; managed background engine refused.
- D3 (`987bb2f87`): Muse Code harness enters without MCP grants and holds the fork execution lease for the whole turn.
- D4 (`dd0c36fa3`, `987bb2f87`): CLI updates through the real install owner, merged with the fork's brew/Caskroom/npm prefix, OMP, Antigravity, leases and Pi `update --all`; compatibility feed points at the fork release base and treats 404 as an empty list.
- D5 (`b8ed9d9c2`): "Native subagents" switch only rewired through the fork `sessions` grant; `RunRequest.mcp` stays empty.
- D6 (`64a26d359`): closing a question without answering records `Some([])` only when it never had an answer.
- D7 (`71033cc96`, `fd7d268ca`, `dfbb6876c`): low-latency terminal, predictive echo, host query replies, device-to-device WebRTC terminal link, new `zeron-term`/`zeron-term-link` crates; terminal stays in the right panel and Workers terminal keeps working.
- Remaining commits per the audit: TAKE/ADAPT for rail, typed project path, PR destination, session card, sender captions, sent-message rows, outbox, Luau, editor highlights, tooltips, subagent gates, provider hardening, Codex approval, Android/landing inert; SKIP for version bumps, commit co-author bot, `4879a4754`, media-only, upstream-only CI workflows and edge admin Cloud (dormant source only).

## Impact

Rust proto/doc/client/engine/rpc/harness/mcp/sessions-mcp/workers-unpeel/update/preview/syntax/ui/mobile, new terminal crates, iOS sources and generated bindings, edge sources (fork runtime and deploy kept), CI scripts and docs. Preserves Workers/`WorkflowTask`, OMP/Kimi, root-only MCP grants, Enter/Steer, Voice, fork accounts/usage, Settings Projects/Accounts, compact transcript without context ring or footer terminal, fork release feed/updater, vendored zui and version 1.0.10.
