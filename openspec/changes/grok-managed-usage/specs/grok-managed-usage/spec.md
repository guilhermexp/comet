# grok-managed-usage Specification

## Purpose

## ADDED Requirements


### Requirement: Read the grok.com CLI credential for Usage

The system SHALL read the Grok OIDC credential from `$GROK_HOME/auth.json` (default `~/.grok/auth.json`) as device-local account state and SHALL NOT treat a `user-settings.json` `apiKey` as a Grok subscription credential. Explicit `AgentAccountsConfig` paths SHALL never fall through to the real user home.

#### Scenario: Valid OIDC entry exists

Test: engine unit test with a temporary `auth.json`.

- **WHEN** `auth.json` contains an entry with non-empty `key`, `refresh_token`, and `oidc_client_id`
- **THEN** the Grok account is eligible for managed Usage refresh
- **AND** the access token, refresh token, and API key never appear in snapshots, warnings, logs, Loro, or edge sync

#### Scenario: Credential is missing or malformed

Test: engine unit table for missing, symlink, over-permissive, and malformed `auth.json` sources.

- **WHEN** the credential file is missing, unsafe, unreadable, or malformed
- **THEN** the Grok account degrades to its no-usage or unavailable state
- **AND** no secret material appears in warnings or logs

### Requirement: Refresh Grok OIDC credentials safely

The system SHALL refresh an expiring Grok access token through the OIDC token endpoint `https://auth.x.ai/oauth2/token` using the entry's own `oidc_client_id`, a cross-process lock on the sibling `auth.json.lock`, a post-lock credential re-read, and atomic `0600` persistence that preserves sibling fields and unrelated entries.

#### Scenario: Another process rotated the credential

Test: concurrent-refresh unit test that rotates the credential before lock acquisition completes.

- **WHEN** Comet acquires the refresh lock and finds a newer refresh token on disk
- **THEN** it uses the newer credential without issuing a duplicate refresh request

#### Scenario: Refresh fails

Test: refresh error table asserting byte-identical persisted credentials and redacted diagnostics.

- **WHEN** refresh returns an authentication, network, timeout, or malformed-response error
- **THEN** Comet preserves the persisted credential file byte-identically
- **AND** reports a redacted provider warning
- **AND** does not expose access or refresh tokens through RPC, UI, logs, Loro, or edge sync

### Requirement: Fetch managed Grok quota windows

The system SHALL request the canonical `GET https://cli-chat-proxy.grok.com/v1/billing?format=credits` origin with bearer authentication and an 8-second timeout, SHALL reject cross-origin redirects, and SHALL normalize `config.currentPeriod` and `config.creditUsagePercent` into a quota window whose label derives from the period type (`Weekly` for `USAGE_PERIOD_TYPE_WEEKLY`, `Monthly` for `USAGE_PERIOD_TYPE_MONTHLY`) and whose reset timestamp is the period end. Tests MAY inject an isolated loopback transport without using production environment overrides or credentials.

#### Scenario: Weekly period is returned

Test: parser unit test covering weekly/monthly period types, fractional percentages, and a payload without a current period.

- **WHEN** the endpoint returns `currentPeriod.type = USAGE_PERIOD_TYPE_WEEKLY` with `creditUsagePercent` and period `end`
- **THEN** Comet maps it to a `Weekly` window with `used_fraction = creditUsagePercent / 100` and `resets_at = end`
- **AND** a payload without a usable current period yields no windows

#### Scenario: Managed endpoint is unavailable

Test: fetch error table for 401, 404, timeout, and invalid JSON/payload.

- **WHEN** the endpoint returns 401, 404, timeout, or an invalid payload
- **THEN** the engine remains operational
- **AND** Grok Usage degrades to a redacted unavailable/no-usage state

#### Scenario: Hostile environment cannot redirect the bearer

Test: configuration unit test proving production ignores or rejects arbitrary base-URL environment values and cross-origin redirects.

- **WHEN** the process environment or a redirect attempts to change the managed Usage origin
- **THEN** Comet does not send the Grok bearer token to the alternate origin
- **AND** reports a redacted unavailable warning

### Requirement: Serve last-known Grok windows across transient failures

The system SHALL retain the last successfully fetched Grok windows keyed by credential fingerprint and SHALL serve them when a forced refresh fails transiently, so a single failed probe does not erase rendered quota. Credential change or removal SHALL invalidate the retained windows.

#### Scenario: Transient fetch failure preserves rendered quota

Test: engine unit test that fetches successfully, fails the next forced fetch, and asserts the previous windows are served.

- **WHEN** a forced refresh fails after a successful fetch for the same credential
- **THEN** the Grok account keeps its last-known windows
- **AND** a redacted warning is reported

#### Scenario: Credential rotation invalidates retained windows

Test: engine unit test rotating `auth.json` after a successful fetch.

- **WHEN** the credential on disk changes after windows were fetched
- **THEN** retained windows for the previous credential are not served

### Requirement: Render Grok in the Usage widget

The Usage widget SHALL reuse the existing quota, reset, pace, and tone presentation for Grok windows; the Grok row SHALL show the weekly remaining percentage summary when a weekly window exists.

#### Scenario: Authenticated subscription has quota data

Test: UI usage-row unit test plus headed GPUI smoke against the authenticated local subscription.

- **WHEN** the Grok snapshot contains a weekly quota window
- **THEN** the Grok summary shows the weekly remaining percentage
- **AND** expanding Grok shows each valid window and its reset information

### Requirement: Keep Grok Usage device-local

The system SHALL keep Grok credentials and quota snapshots outside session documents and multi-device synchronization.

#### Scenario: Snapshot crosses engine/UI RPC

Test: engine snapshot test proving normalized fields only.

- **WHEN** Grok usage is refreshed
- **THEN** only normalized account and quota fields cross the device-local engine/UI RPC boundary
- **AND** credentials never enter serialized account snapshots
