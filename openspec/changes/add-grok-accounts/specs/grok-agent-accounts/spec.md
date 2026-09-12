## Purpose

Device-local detection of the grok.com CLI subscription (`grok login`) so Grok appears in Settings → Accounts as managed identity, not as a console.x.ai API key.

## ADDED Requirements

### Requirement: Detect the grok.com CLI login as a managed account

The system SHALL treat a `~/.grok/auth.json` (or `$GROK_HOME/auth.json`) entry with a non-empty `email` and session `key` as the live Grok CLI subscription. Explicit `AgentAccountsConfig` paths SHALL never fall through to the real user home. The snapshot SHALL expose a non-switchable Grok account with `auth_kind` OAuth and plan `Managed`. The snapshot SHALL NOT include the session token, refresh token, or any `user-settings.json` `apiKey`.

#### Scenario: grok login is present

Test: engine unit test with a temporary `auth.json`.

- **WHEN** `auth.json` contains an OIDC entry with email and session key
- **THEN** the snapshot contains one active, non-switchable Grok account
- **AND** the email is the grok.com login email
- **AND** a `user-settings.json` `apiKey` does not appear as an account
- **AND** the serialized snapshot does not contain the session token or API key

#### Scenario: Missing grok login

Test: engine unit test with missing `auth.json`.

- **WHEN** `auth.json` is missing
- **THEN** the snapshot contains no Grok account even if `user-settings.json` has an `apiKey`

### Requirement: Render Grok in Settings → Accounts

Settings → Accounts SHALL include Grok after Cursor in the fixed provider order. The section SHALL NOT offer Add account. The empty state SHALL say that no Grok subscription was detected and to run `grok login`.

#### Scenario: Provider order and no add action

Test: UI unit test on `PROVIDERS` and `provider_can_add`.

- **WHEN** the Accounts page renders providers
- **THEN** Grok is last after Cursor
- **AND** `provider_can_add(Grok)` is false
