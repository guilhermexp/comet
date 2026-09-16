## 1. Persist hidden account ids

- [x] 1.1 Add `usage_widget_hidden_account_ids: BTreeSet<String>` to `UiSettings` with empty default, serde skip-if-empty, helpers to get/set visibility, and no copy in `apply_shell_settings`
- [x] 1.2 Add unit tests for round-trip, omitted-field default, and shell-save isolation

## 2. Derive Usage rows from visible accounts

- [x] 2.1 Change `provider_usage_rows` to take the hidden set, emit one row per visible account in Accounts provider order, include Cursor, and keep one not-signed-in placeholder per provider with no account in the snapshot
- [x] 2.2 Key expand/id by account id; attach email/display_name only when multiple visible accounts share a harness
- [x] 2.3 Update existing usage tests and add hidden/two-Claude/Cursor/empty-membership cases

## 3. Wire Accounts toggle and sidebar paint

- [x] 3.1 Add trailing `toggle_switch` on each Accounts row; Immediate save + `refresh_windows`
- [x] 3.2 Re-derive Usage rows at paint from cached snapshot + `settings::current`; empty-state copy when zero rows
- [x] 3.3 Update `crates/ui/AGENTS.md` Accounts/Usage contracts and coverage matrix

## 4. Verify

- [x] 4.1 `cargo test -p zeron-ui accounts usage settings --lib`
- [x] 4.2 `openspec validate account-usage-widget-visibility --strict --no-interactive`
