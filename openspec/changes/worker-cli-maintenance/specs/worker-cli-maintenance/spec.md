## ADDED Requirements

### Requirement: Detect installed worker CLI versions and install source

The app SHALL detect the installed version of supported worker CLIs by running their `--version`
command and parsing semantic version output. The app SHALL classify the install source
(npm, bun, pnpm, homebrew, native, unknown) by inspecting the resolved path.

#### Scenario: Installed CLI detects version successfully

Test: `detects_installed_cli_version`

- **WHEN** a worker CLI binary exists on PATH and returns parseable semver output
- **THEN** the app records its `current_version`
- **AND** identifies its install source

#### Scenario: Missing binary does not fail or throw

Test: `missing_cli_binary_reported_cleanly`

- **WHEN** a worker CLI binary cannot be found on PATH
- **THEN** it is marked as not installed without erroring or blocking startup

### Requirement: Query latest published version with caching

The app SHALL query the latest available version from npm registry or Homebrew APIs
with a bounded timeout and an in-memory cache with a 1-hour TTL.

#### Scenario: Latest version query succeeds and is cached

Test: `queries_and_caches_latest_version`

- **WHEN** the registry API returns the latest version
- **THEN** the app records `latest_version` and serves subsequent queries from cache within the TTL

#### Scenario: Registry query failure is handled gracefully

Test: `registry_query_failure_yields_unknown_without_error`

- **WHEN** network request fails or times out
- **THEN** `latest_version` is empty and advisory status is `Unknown` without throwing

### Requirement: Generate version advisory and update command

The app SHALL compare `current_version` against `latest_version` using semver
and produce an advisory indicating whether the CLI is current, behind latest, or unknown,
along with the appropriate update command.

#### Scenario: CLI is behind latest version

Test: `cli_behind_latest_generates_update_advisory`

- **WHEN** `current_version` is semver-older than `latest_version`
- **THEN** status is `BehindLatest`
- **AND** `can_update` is true when a safe one-click update command exists
- **AND** `update_command` matches the detected install source

#### Scenario: CLI is up to date

Test: `cli_up_to_date_generates_current_status`

- **WHEN** `current_version` equals or exceeds `latest_version`
- **THEN** status is `Current`
- **AND** no update is prompted

### Requirement: Execute worker CLI update

The app SHALL execute the update command for a CLI with concurrency locks
and timeout bounds, re-checking the installed version afterwards.

#### Scenario: Update command succeeds

Test: `update_command_succeeds_and_refreshes_status`

- **WHEN** the update command completes successfully and the new version is no longer behind
- **THEN** outcome is `Succeeded`
- **AND** advisory status updates to `Current`

#### Scenario: Update command completes but version remains unchanged

Test: `update_command_unchanged_provides_manual_fallback`

- **WHEN** the update command runs but the detected version remains behind latest
- **THEN** outcome is `Unchanged`
- **AND** a copyable manual command is provided

### Requirement: Floating bottom-right startup update toast

When one or more installed worker CLIs are behind the latest version on startup,
the app SHALL display a floating toast notification in the bottom-right corner of the window
offering `Review` and `Update all`, de-duplicated per app session and auto-dismissing after a duration.

#### Scenario: Updates available shows bottom-right toast once per session

Test: `startup_shows_bottom_right_toast_once_per_session`

- **WHEN** background startup check finds CLIs behind latest and notification has not been shown this session
- **THEN** a bottom-right toast appears with the count of updates and actions
- **AND** clicking `Review` opens Workers Presets settings
- **AND** clicking `Update all` runs sequential updates
- **AND** subsequent checks in the same session do not show the toast again

### Requirement: Workers Presets settings displays version status and update actions

The Workers Presets panel SHALL display the installed version and update affordance for each CLI,
including `current → latest` delta and `Update` buttons, as well as `Update all` and `Check for updates` controls.

#### Scenario: Presets panel displays outdated CLI with update button

Test: `presets_panel_displays_outdated_cli_with_update`

- **WHEN** a preset corresponds to an installed CLI that is behind latest
- **THEN** the row displays `v{current} → v{latest}` and an `Update` button
- **AND** clicking `Update` triggers the update with a visual progress indicator
