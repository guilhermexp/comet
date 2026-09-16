## Why

Settings → Accounts already lists every device-local provider login with its Managed Provider Usage meters, but the Usage widget ignores that list: it hardcodes four providers and shows only the `active` account of each. The two surfaces are separate, so a spare Claude slot, an Antigravity pool member, or Cursor never appears unless it happens to be the active pick — and there is no way to hide a provider from the widget without forgetting the account.

## What Changes

- Each Accounts row gets a trailing toggle that opts that login into (default ON) or out of the Usage widget.
- The Usage widget derives one row per visible account, in Accounts provider order then engine slot order. Multiple logins of the same harness can appear. Cursor is eligible. Hidden accounts stay on the Accounts page with their meters.
- Hiding every account of a provider removes it from the widget; an empty widget is valid. A provider with no account in the snapshot at all keeps a `NotSignedIn` placeholder — see `always-show-usage-providers`, which split the two cases after the first shipped behavior (omit both) read as Comet losing providers it had never been told to hide.
- Visibility is a device-local `UiSettings` set of hidden account ids. No engine, RPC, CRDT, or snapshot field.

## Capabilities

### New Capabilities

- `account-usage-widget-visibility`: Per-account opt-in that makes Settings → Accounts the membership source for the Usage widget.

### Modified Capabilities

- `kimi-managed-usage`: Kimi membership follows the same per-account toggle; a hidden Kimi account leaves the widget, while an absent credential keeps the placeholder row.
- `antigravity-managed-usage`: Widget membership follows the same per-account toggle; Antigravity is no longer guaranteed to be the fourth row.
- `usage-widget-freshness-and-tone`: Row derivation takes the hidden-id set; countdown/tone rules still apply per remaining visible row.

## Impact

- `crates/ui/src/settings.rs`: persist `usage_widget_hidden_account_ids`.
- `crates/ui/src/settings/accounts.rs`: toggle on each account row.
- `crates/ui/src/details_sidebar/usage.rs`: one row per visible account.
- `crates/ui/src/details_sidebar/view.rs`: re-derive from snapshot + current settings; expand keys by account id.
- DOX: `crates/ui/AGENTS.md` Accounts/Usage contracts.
