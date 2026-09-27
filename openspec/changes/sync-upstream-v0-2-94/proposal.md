## Why

Upstream `zeronsh/zeron` moved from the last synced base (`be5bdc41`, v0.2.83 line) to `433aa148` (v0.2.94 + iOS follow-ups): 67 commits, ~225 desktop files plus the iOS rewrite. It carries real fixes the fork lacks (chat-history resource exhaustion, active-chat connection starvation, dead-token re-login, discard safety) and moves the desktop markdown parser into a shared crate the iOS core also uses.

## What Changes

- Real merge of `refs/upstream/zeron-main@433aa148`, so the next sync starts from v0.2.94.
- Taken as-is or lightly adapted: chat file links jump to lines (#524), I-beam over chat text (#529), system proxy (#504), boot splash skip (#553), project actions during load (#548), Copy Path (#245), session shortcuts from Settings, projectless file explorer (#508, #457), most-recent right tab on close (#571), focus-aware tab navigation (#572), Cursor SDK pin.
- Accounts: re-login revives the live account and the live account can be removed (#546); Pi account store, `provider` field and secret redaction from #542; engine-side usage cache/backoff from #449.
- Sync stability: history resource budget (#544), focus-fair connection rotation (#550), no transient notices for dormant chats (#552), ported over the fork's durable outbox.
- Adapted to fork presentation: diff header "open in Files" (#311), discard safety guards from #81 in the fork `discard_files`, new-chat canvas terminals (#474) keeping the fork's `chatId` xor `cwd` rule, last Settings section (#541).
- Side chats core from #498/#568/#567: `MessagePart::Fork`, `ForkSideChat`, batch MCP chat tools. UI stays in fork surfaces; upstream's always-on Zeron MCP injection is not taken.
- iOS: upstream's Rust-core rewrite replaces the fork's SwiftUI app; `crates/markdown`, `crates/text`, `crates/mobile` enter the workspace.
- Not taken: Settings redesign UI (#449), plan-usage ring (#547), New project palette (#549), PR badge restyle, OpenCode auto-approve (#533), update check without workspace sync (#535), Windows-only fixes, Explorer Subagents/Chats sections.

## Capabilities

### New Capabilities

- `upstream-v0-2-94-integration`: fork contracts preserved across the v0.2.94 merge.

### Modified Capabilities

None at the spec level; behavior changes are recorded in the owning `AGENTS.md` files.

## Impact

Most of the workspace: `crates/{engine,rpc,sync,doc,proto,harness,mcp,ui,update}`, new `crates/{markdown,text,mobile}`, `apps/ios`, `edge/` (nudge ACK), `Cargo.toml`/`Cargo.lock`.
