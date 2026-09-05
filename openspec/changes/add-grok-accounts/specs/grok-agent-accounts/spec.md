## Purpose

Device-local detection and slot management for the grok CLI API key so Grok appears in Settings → Accounts like other switchable logins.

## ADDED Requirements

### Requirement: Detect the live Grok API key as an account

The system SHALL treat a non-empty `apiKey` in `${GROK_HOME:-~/.grok}/user-settings.json` as the live Grok login. Explicit `AgentAccountsConfig` paths SHALL never fall through to the real user home. The account snapshot SHALL expose a switchable Grok account with `auth_kind` API key and SHALL NOT include the raw key in serialized snapshots, warnings, or logs.

#### Scenario: Valid API key is present
Test: engine unit test with a temporary `user-settings.json`.

- **WHEN** `user-settings.json` contains a non-empty `apiKey`
- **THEN** the snapshot contains one active, switchable Grok account
- **AND** the display label is a truncated API-key hint, not the full key
- **AND** the serialized snapshot does not contain the raw key

#### Scenario: Missing or empty API key
Test: engine unit test with missing file and empty `apiKey`.

- **WHEN** the settings file is missing or `apiKey` is empty
- **THEN** the snapshot contains no Grok account

### Requirement: Switch Grok slots without clobbering other settings

Activating a saved Grok slot SHALL write only `apiKey` into the live `user-settings.json` and SHALL preserve sibling fields such as `defaultModel`. Forget SHALL refuse the live login and delete only inactive slot files.

#### Scenario: Activate restores the key and keeps defaultModel
Test: engine unit test swapping two keys.

- **WHEN** two Grok keys have been snapshotted and the user activates the earlier slot
- **THEN** live `user-settings.json` contains that slot's `apiKey`
- **AND** `defaultModel` is unchanged

### Requirement: Render Grok in Settings → Accounts

Settings → Accounts SHALL include Grok after Cursor in the fixed provider order. The section SHALL NOT offer Add account. The empty state SHALL say that no Grok API key was detected on this device.

#### Scenario: Provider order and no add action
Test: UI unit test on `PROVIDERS` and `provider_can_add`.

- **WHEN** the Accounts page renders providers
- **THEN** Grok is last after Cursor
- **AND** `provider_can_add(Grok)` is false
