## Context

See `proposal.md`. Trial merge (`git merge-tree origin/main 433aa148`): 73 conflicted files, 13 of them iOS. Four read-only audits (Explorer/MCP, accounts/settings, engine stability, UI) classified every non-landing commit.

## Decisions

### D1. Real merge, fork wins on rejected features

Same policy as `sync-upstream-v0-2-83` D2/D4: combine hunk by hunk; rejected upstream features are resolved to the fork side inside the merge so the next sync does not see them again.

### D2. Update feed

The fork's default edge is upstream's `edge.zeron.sh`, whose `/releases` ships Zeron. Fixed before the merge (`zeron-update` refuses `zeron.sh` hosts without `ZERON_RELEASES_URL`). Upstream #535 (check even without workspace sync) is not taken.

### D3. Zeron MCP injection

Upstream injects `zeron mcp` into every run through `RunRequest.mcp`. The fork already injects `comet-workers`/`comet-sessions` through `harness::workers_mcp` with a root-orchestrator grant. The `mcp` field exists for compatibility but the engine leaves it empty; batch chat tools and side-chat guards land in the fork's MCP server.

### D4. Side chats

Core only: `MessagePart::Fork` divider, `ForkSideChat` RPC with the fork-history bootstrap, batch MCP tools. Upstream's Explorer footer sections (`files/sections.rs`) duplicate Workers › Subagents and are not taken.

### D5. Accounts and Settings

Settings UI stays the fork's. From #449 only engine behavior: stale-while-revalidate usage cache, refresh only on 401/403, Retry-After backoff, login URL validation. From #542 only Pi, the `provider` field and redaction. The fork's Grok stays.

### D6. Sync stability chain

#544 → #550 → #552 are ported together over the fork `doc_host` outbox. The fork's `MAX_CONCURRENT_DIALS` semaphore is replaced by upstream's shared budget, not kept alongside it.

### D7. iOS rewrite accepted

Upstream deleted the SwiftUI app. Keeping it would make every future iOS sync manual, so the rewrite is taken and fork iOS deltas (OMP identity, streaming tweaks) are re-evaluated against the new app later. `crates/markdown` is required by desktop regardless.

## Risks

- No gpui render harness: visual smoke is required before promotion.
- The sync stability port touches the relay/nudge protocol; the production edge is upstream's and already speaks the ACK.
- iOS: no simulator runtime on this Mac; `xcodebuild test` runs in CI only.
