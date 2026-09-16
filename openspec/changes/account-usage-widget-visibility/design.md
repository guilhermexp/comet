## Context

See proposal.md for motivation. Today `provider_usage_rows` walks four hardcoded harnesses and keeps `min_by_key(|a| !a.active)`. Accounts already lists every slot, including inactive Claude/Codex backups and Cursor. Usage is device-local UI; `UiSettings` is the existing home for durable display choices. `DetailsSidebarPreferences.hidden` hides whole widgets, not accounts.

## Goals / Non-Goals

**Goals:**
- Accounts is the membership source for Usage.
- Default ON, persist opt-out ids, filter at derivation.
- Immediate widget update from the cached snapshot.

**Non-Goals:**
- Engine/RPC field on `AgentAccount`.
- Skipping usage probes for hidden accounts.
- Cross-device sync of the hidden set.
- Changing login, switch, forget, or meter rendering on Accounts.

## Decisions

1. **Hidden-id set, not a boolean map.** Missing id = visible. New logins appear without a migration. Forgotten ids left in the set are inert.
2. **`BTreeSet<String>` on `UiSettings`.** Deterministic JSON. `#[serde(default, skip_serializing_if = "BTreeSet::is_empty")]`. Not copied by `apply_shell_settings`.
3. **One row per visible account.** Drop the active-only pick. Order = `accounts::PROVIDERS` then engine slot order via `provider_accounts`.
4. **Re-derive at paint.** Cache the snapshot, not filtered rows. Toggle writes `SavePolicy::Immediate` and calls `cx.refresh_windows()` so the sidebar rereads `settings::current`.
5. **Expand/id key = account id.** Provider label collision is real once two Claudes can show. Show email/display_name only when more than one visible account shares the harness.
6. **Keep `NotSignedIn` as the placeholder for an undetected provider only.** The gate is the pre-filter account list: no account in the snapshot → one placeholder row; accounts present but all hidden → no row, because the toggle is an explicit instruction and must be visible in its effect. The widget can still be empty, so the empty-state copy stays. (Revised by `always-show-usage-providers`; this change first shipped no placeholder in either case.)

## Risks / Trade-offs

- Inactive saved slots become visible by default. That matches the chosen per-account model; users hide them with the toggle.
- `refresh_windows` is coarser than observing SettingsStore. Same pattern as appearance/typography; avoids a new global observer.
- Providers with no device-local credential are visible by default as placeholders and cannot be toggled away individually, since there is no account id to hide. Accepted: the Usage card itself is toggleable from the Details gear.

## Migration

No migration. Absent field deserializes empty → previous accounts stay visible, plus inactive/Cursor slots that the old widget dropped. That is the approved default.
