## ADDED Requirements

### Requirement: One agent CLI updater

The engine's `HarnessUpdateCoordinator` SHALL be the only component that checks for and installs agent CLI updates. Every UI surface that offers an agent update SHALL read the coordinator's `HarnessUpdateStatus` rows and act through `CHECK_HARNESS_UPDATES` and `APPLY_HARNESS_UPDATE`.

#### Scenario: Workers Presets follow the coordinator

Test: `presets_map_their_cli_to_the_upstream_updater_row`

- **WHEN** a preset's CLI maps to a monitored harness and its row is `Available`
- **THEN** the preset shows `v{installed} → v{latest}`
- **AND** its **Update** button applies that harness's update through the engine
- **AND** a CLI without a harness (`agy`) shows no update badge

### Requirement: Package-manager updates for Codex

When Codex has no standalone install, the coordinator SHALL update it through the package manager that owns the resolved executable. It SHALL resolve that package manager from the install itself, never from `PATH`.

#### Scenario: npm and Homebrew installs are applicable

Test: `codex_updates_through_the_package_manager_that_owns_the_install`

- **WHEN** Codex resolves under `<prefix>/lib/node_modules/@openai/codex/` and `<prefix>/bin/npm` exists
- **THEN** the plan runs `<prefix>/bin/npm install -g --prefix <prefix> @openai/codex@latest`
- **WHEN** Codex resolves under `<brew>/Caskroom/codex/` and `<brew>/bin/brew` exists
- **THEN** the plan runs `<brew>/bin/brew upgrade --cask codex`
- **AND** any other location stays a manual update

### Requirement: OMP is monitored

The coordinator SHALL monitor OMP. It SHALL read the latest version from npm `@oh-my-pi/pi-coding-agent` and install with `omp update`.

#### Scenario: OMP is applicable

Test: `omp_is_tracked_and_updates_itself`

- **WHEN** OMP is enabled
- **THEN** it is monitored and its update is applicable
- **AND** Kimi remains unmonitored

### Requirement: Update all from the Home island

When two or more rows are connected, `Available` and applicable, the Home update island SHALL offer **Update all**, which applies each of those rows.

#### Scenario: Only applicable rows are installed

Test: `update_all_installs_only_connected_applicable_rows`

- **WHEN** the island lists applicable, manual, in-progress and disconnected rows
- **THEN** **Update all** targets only the connected, `Available`, applicable rows
