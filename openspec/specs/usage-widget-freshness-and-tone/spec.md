# usage-widget-freshness-and-tone Specification

## Purpose

Live usage countdowns, periodic background refresh, and visual quota styling in the Usage widget.

## Requirements

### Requirement: Maintain live usage countdowns and periodic background refresh

The Usage widget SHALL re-derive countdown strings and pace indicators from the cached snapshot on a 30-second local tick without network I/O, and SHALL periodically refetch agent accounts over RPC when at least 120 seconds have elapsed since the last successful fetch. A failed periodic refresh SHALL preserve the previously loaded snapshot and rows while exposing their freshness and any known refresh failure. RPC success SHALL NOT imply successful quota collection for every account.

#### Scenario: Countdown strings advance on local tick
Test: none — o ticker é ciclo de vida de entidade gpui (`Task` + `cx.spawn`) e não tem harness de render; validação é visual (`scripts/dev-demo.sh`). A formatação por `now` que ele reaplica é coberta por `cargo test -p zeron-ui usage` (`reset_countdown_drops_an_empty_hours_segment`, `weekly_reset_badge_*`).

- **WHEN** the local 30-second ticker fires with a cached snapshot
- **THEN** the usage rows are re-derived with fresh `now` timestamps
- **AND** reset countdowns and ETA strings reflect elapsed time without issuing an RPC request

#### Scenario: Periodic network refetch preserves existing snapshot on failure
Test: unit — derivação das rows em `crates/ui/src/details_sidebar/usage.rs`; integração do carregamento em fixture UI segura.

- **WHEN** a background refetch is triggered after 120 seconds and the RPC call fails
- **THEN** the existing `usage_snapshot` and derived rows are retained
- **AND** the widget retains its rows without flashing an empty state or a global error in place of the accounts
- **AND** each retained quota preserves its collection time and becomes visibly stale when its freshness budget expires

#### Scenario: RPC succeeds but one account collection fails
Test: unit + integration — fixtures de contas na engine e derivação das rows na UI.

- **WHEN** an accounts RPC succeeds with one account's old quota and a refresh failure
- **THEN** that account is shown as stale immediately
- **AND** other accounts with successful fresh quota collections remain current
- **AND** the RPC response time does not replace the quota collection time

### Requirement: Emphasize weekly header summary and badge by remaining quota

The Usage widget SHALL derive a `UsageTone` (`Neutral`, `Warning`, `Danger`) for each provider row based on the remaining percentage of its weekly quota window.

#### Scenario: Tone boundaries for weekly quota
Test: UI unit test in `usage.rs` testing boundary fixtures at 0%, 1%, 15%, 16%, 50%, and 51% remaining.

- **WHEN** weekly remaining quota is 0% (exhausted)
- **THEN** the `weekly_tone` is `Neutral`
- **WHEN** weekly remaining quota is between 1% and 15% inclusive
- **THEN** the `weekly_tone` is `Danger`
- **WHEN** weekly remaining quota is between 16% and 50% inclusive
- **THEN** the `weekly_tone` is `Warning`
- **WHEN** weekly remaining quota is 51% or higher
- **THEN** the `weekly_tone` is `Neutral`

#### Scenario: Tone for missing weekly window or non-ready states
Test: UI unit test in `usage.rs` for `NotSignedIn`, `NoUsage`, and accounts without a weekly window.

- **WHEN** an account has no weekly window or is in `NotSignedIn` or `NoUsage` state
- **THEN** the `weekly_tone` is `Neutral`

### Requirement: Display reset countdown badge when quota is exhausted

The Usage widget SHALL display the weekly reset countdown badge when the weekly window has a future reset timestamp and either the reset is within 48 hours or the remaining quota is 0%.

#### Scenario: Reset countdown visible when quota is exhausted
Test: UI unit test in `usage.rs` with 0% remaining and reset 5 days in the future.

- **WHEN** the weekly remaining quota is 0% and `resets_at` is in the future beyond 48 hours
- **THEN** `weekly_reset_badge` is `Some` formatted with the reset duration (e.g. `Reset 5d 0h`)

#### Scenario: Distant reset badge hidden when quota remains
Test: UI unit test in `usage.rs` with 60% remaining and reset 5 days in the future.

- **WHEN** the weekly remaining quota is greater than 0% and `resets_at` is beyond 48 hours
- **THEN** `weekly_reset_badge` is `None`

#### Scenario: Reset badge absent without future reset timestamp
Test: UI unit test in `usage.rs` with 0% remaining and no `resets_at` or past `resets_at`.

- **WHEN** `resets_at` is `None` or in the past
- **THEN** `weekly_reset_badge` is `None`

### Requirement: Quota identity matches the provider account

The system SHALL validate a Codex quota response against the provider account/workspace associated with the requested credentials. A composite local account/cache key SHALL NOT be compared directly to a provider workspace-only identifier. A genuinely foreign response SHALL NOT update any account's quota or be reported as a missing local credential.

#### Scenario: Composite local identity and valid provider response
Test: unit + integration — fixtures de contas e resposta HTTP em `crates/engine/src/agent_accounts.rs`.

- **WHEN** a local slot uses a user-and-workspace composite key and the authenticated quota response identifies the matching provider workspace
- **THEN** the collection succeeds and updates that slot's quota and timestamp
- **AND** the valid response is not discarded as missing credentials

#### Scenario: Provider response belongs to another identity
Test: unit + integration — fixtures de conta Codex na engine.

- **WHEN** the returned provider identity differs from the one expected for the requested credentials
- **THEN** the response is rejected without changing either account's last valid quota
- **AND** the account exposes a failed or unverified collection rather than silently serving a current-looking old balance

### Requirement: Retained quota is visibly historical when freshness is unproven

The Usage widget SHALL mark quota stale when collection failed or its last valid collection is older than five minutes. Missing or future collection times SHALL be treated as freshness unknown. The collapsed row SHALL expose stale/unknown state; retained percentages and reset estimates SHALL be identified as historical in the detail, with the last valid collection time when known.

#### Scenario: Old weekly quotas survive repeated failures
Test: unit + UI render fixture — `crates/ui/src/details_sidebar/usage.rs` e render do painel Details.

- **WHEN** two Codex rows retain 74% and 96% remaining from days earlier and refresh is failing
- **THEN** both collapsed rows visibly indicate stale data rather than presenting those percentages as current remaining quota
- **AND** expanded details identify the historical readings and their collection times
- **AND** an expired historical reset does not fabricate a new current balance or reset countdown

#### Scenario: Old or unknown timestamps without an error message
Test: unit — derivação por `now` em `crates/ui/src/details_sidebar/usage.rs`.

- **WHEN** quota has no error text but is older than five minutes, lacks a collection timestamp or has a timestamp later than the observing clock
- **THEN** it is stale or of unknown freshness, respectively
- **AND** a recent successful accounts RPC does not relabel it current

### Requirement: Per-account quota failures have actionable recovery

The system SHALL distinguish authentication/credential failures from transient quota-refresh failures for each Codex account. Missing credentials SHALL NOT silently hide a failed collection behind retained quota. Recovery SHALL use existing account controls, preserve the active account and neighboring credential data, respect retry limits and clear stale/error state only after a successful collection for that identity.

#### Scenario: Expired saved account has historical quota
Test: unit + integration — engine account probe fixtures e UI account-row fixtures.

- **WHEN** one saved Codex account returns 401 while another can fetch quota
- **THEN** the failing row exposes the authentication failure and points to the existing account recovery control
- **AND** viewing or refreshing Usage does not switch the active Codex identity or copy a quota to another account

#### Scenario: Credentials become available again
Test: unit + integration — `crates/engine/src/agent_accounts.rs` e fixtures de conta Codex.

- **WHEN** credentials for an account that previously failed become valid and a permitted refresh succeeds
- **THEN** only that account's quota and collection timestamp are updated and its failure/freshness state is cleared
- **AND** stale in-flight results from a previous credential generation cannot overwrite the recovered snapshot
- **AND** unchanged rejected credentials remain subject to backoff rather than repeated login or refresh loops

#### Scenario: Successful source and displayed window agree
Test: integration — resposta HTTP sanitizada em fixture da engine e derivação UI.

- **WHEN** a successful Codex quota read reports 5% consumed in a 604800-second window for an account
- **THEN** that account's weekly current balance is 95% remaining
- **AND** the UI does not substitute a short-term window, another account's balance or a local token-activity total
