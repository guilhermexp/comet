## Context

See proposal.md. grok 1.1.x stores `apiKey` in `user-settings.json`; there is no `grok login`. Comet's ACP harness still launches `grok agent stdio` when that binary exists; Accounts follows the file the installed CLI actually reads.

## Goals / Non-Goals

**Goals:** detect, snapshot, switch, forget, list in Accounts, eligible for Usage.

**Non-Goals:** OAuth/browser login, reading `wallet.json` or `XAI_API_KEY`, quota probes, changing the ACP harness.

## Decisions

1. **File is the source of truth.** `$GROK_HOME/user-settings.json` `apiKey`, same env relocation as Codex.
2. **Slot stores `{apiKey}` only.** Activate merges that field so `defaultModel`/`payments` stay live.
3. **No Add.** `provider_can_add` is false. A new key typed into grok is detected on the next list.
4. **Truncated label.** `API key ·…{last4}`; hash of the key is `account_key`. Raw key never crosses RPC.

## Risks / Trade-offs

- Two different grok binaries exist in the wild (Grok Build ACP vs Bun TUI). This change follows the TUI key file present on this device. If Grok Build later grows its own store, detect it separately.
