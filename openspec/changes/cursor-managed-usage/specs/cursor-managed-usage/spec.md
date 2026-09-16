# cursor-managed-usage Specification

## ADDED Requirements

### Requirement: Read a Cursor session token for Usage from either device-local login

The system SHALL read the Cursor subscription Usage credential from whichever of two device-local logins holds one, trying them in this order:

1. the Cursor desktop app's local storage (`state.vscdb`, keys `cursorAuth/accessToken` and `cursorAuth/cachedEmail`);
2. the `cursor-agent` CLI login in the macOS Keychain (service `cursor-access-token`, account `cursor-user`), whose account email is read from `~/.cursor/cli-config.json` (`authInfo.email`).

The system SHALL NOT treat the `cursor-agent` SDK API key (`crsr_…`) as a subscription quota credential; the quota endpoint rejects it with `401 ERROR_NOT_LOGGED_IN`. Both stores SHALL be read-only — the state database is never written, vacuumed, or otherwise mutated, and the Keychain item is only read. Explicit `AgentAccountsConfig` paths and the test constructor SHALL never fall through to the real user home or to the real Keychain login.

#### Scenario: Desktop session exists

Test: engine unit test with a temporary SQLite `ItemTable`.

- **WHEN** `state.vscdb` contains a non-empty `cursorAuth/accessToken`
- **THEN** the Cursor account is eligible for managed Usage probing
- **AND** the desktop token is preferred over the CLI login, since Cursor's own app keeps it fresh
- **AND** the access token never appears in snapshots, warnings, logs, Loro, or edge sync

#### Scenario: Only the CLI is logged in

Test: none — reading the real Keychain needs a real `cursor-agent` login; covered by a manual engine probe on a device with the CLI and no desktop app.

- **WHEN** the desktop store is absent or unreadable and `cursor-agent` holds a Keychain login
- **THEN** the Cursor account is eligible for managed Usage probing with the CLI token
- **AND** the quota attaches to the account named by `authInfo.email`, or to the active Cursor account when that file names none

#### Scenario: Neither login is present or readable

Test: engine unit table for missing database, missing key, and unreadable database, with the CLI Keychain source disabled.

- **WHEN** neither store yields a usable token
- **THEN** the Cursor account degrades to its no-usage or unavailable state
- **AND** no secret material appears in warnings or logs

### Requirement: Fetch managed Cursor quota windows

The system SHALL request `POST {backend}/aiserver.v1.DashboardService/GetCurrentPeriodUsage` (production backend `https://api2.cursor.sh`) with bearer authentication, `Connect-Protocol-Version: 1`, a JSON `{}` body, and an 8-second timeout, SHALL reject cross-origin redirects, and SHALL normalize `planUsage.totalPercentUsed` (falling back to `autoPercentUsed`, then `totalSpend / limit`) into a quota window labeled `Monthly` whose used fraction is clamped to `[0,1]` (an exhausted plan reports over 100% and every consumer renders this as a bar width) and whose reset timestamp is `billingCycleEnd`.

#### Scenario: Plan usage payload parses

Test: parser unit table over recorded response shapes.

- **WHEN** the response carries `planUsage.totalPercentUsed` plus a disagreeing `totalSpend` / `limit`, and `billingCycleEnd` in epoch milliseconds
- **THEN** one `Monthly` window is produced with `used_fraction = totalPercentUsed / 100`, clamped to `[0,1]`, and `resets_at = billingCycleEnd`

#### Scenario: Spend ratio is only a fallback

Test: parser unit table over a payload that has spend/limit and no percent fields.

- **WHEN** the response carries `planUsage` with `totalSpend` and a positive `limit`, and no `totalPercentUsed` / `autoPercentUsed`
- **THEN** one `Monthly` window is produced with `used_fraction = totalSpend / limit`, clamped to `[0,1]`

#### Scenario: Authentication or transport failure

Test: engine unit error table for 401, 403, 5xx, timeout, and invalid JSON.

- **WHEN** the fetch fails with an authentication, network, timeout, or malformed-response error
- **THEN** the account reports a redacted provider warning
- **AND** last-known-good windows for the same session token are served when present
- **AND** no token material appears in the warning, logs, RPC, Loro, or edge sync

### Requirement: Attach Cursor usage to the matching account

Settings → Accounts and the Usage widget SHALL show Cursor quota on the Cursor account whose email matches the desktop session's `cachedEmail`; when the session carries no email, the quota attaches to the active Cursor account. Quota never attaches to an account whose email contradicts the session email.

#### Scenario: Emails match

Test: `agent_accounts` snapshot test with a temporary SQLite store and a mock usage server.

- **WHEN** the active Cursor slot's email equals the session's `cachedEmail`
- **THEN** that account's `usage_windows` carry the probed `Monthly` window
- **AND** the serialized snapshot contains no token material

#### Scenario: Emails differ

Test: `agent_accounts` snapshot test with a mismatched session email.

- **WHEN** the session's `cachedEmail` names a different account than the active Cursor slot
- **THEN** no Cursor account receives those windows
