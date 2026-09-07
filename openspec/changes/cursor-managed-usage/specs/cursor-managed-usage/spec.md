# cursor-managed-usage Specification

## ADDED Requirements

### Requirement: Read the Cursor desktop session token for Usage

The system SHALL read the Cursor subscription Usage credential from the Cursor desktop app's local storage (`state.vscdb`, keys `cursorAuth/accessToken` and `cursorAuth/cachedEmail`) as device-local account state, and SHALL NOT treat the `cursor-agent` SDK API key (`crsr_…`) as a subscription quota credential. The state database SHALL be opened read-only, never written, vacuumed, or otherwise mutated. Explicit `AgentAccountsConfig` paths SHALL never fall through to the real user home.

#### Scenario: Desktop session exists

Test: engine unit test with a temporary SQLite `ItemTable`.

- **WHEN** `state.vscdb` contains a non-empty `cursorAuth/accessToken`
- **THEN** the Cursor account is eligible for managed Usage probing
- **AND** the access token never appears in snapshots, warnings, logs, Loro, or edge sync

#### Scenario: Desktop session is missing or unreadable

Test: engine unit table for missing database, missing key, and unreadable database.

- **WHEN** the database or the `cursorAuth/accessToken` row is missing, unreadable, or malformed
- **THEN** the Cursor account degrades to its no-usage or unavailable state
- **AND** no secret material appears in warnings or logs

### Requirement: Fetch managed Cursor quota windows

The system SHALL request `POST {backend}/aiserver.v1.DashboardService/GetCurrentPeriodUsage` (production backend `https://api2.cursor.sh`) with bearer authentication, `Connect-Protocol-Version: 1`, a JSON `{}` body, and an 8-second timeout, SHALL reject cross-origin redirects, and SHALL normalize `planUsage.totalSpend` / `planUsage.limit` into a quota window labeled `Monthly` whose reset timestamp is `billingCycleEnd`.

#### Scenario: Plan usage payload parses

Test: parser unit table over recorded response shapes.

- **WHEN** the response carries `planUsage` with `totalSpend` and a positive `limit`, plus `billingCycleEnd` in epoch milliseconds
- **THEN** one `Monthly` window is produced with `used_fraction = totalSpend / limit` and `resets_at = billingCycleEnd`

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
