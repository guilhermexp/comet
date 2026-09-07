# usage-widget-freshness-and-tone delta (cursor-managed-usage)

## MODIFIED Requirements

### Requirement: Emphasize weekly header summary and badge by remaining quota

The Usage widget SHALL derive a `UsageTone` (`Neutral`, `Warning`, `Danger`) for each provider row based on the remaining percentage of its primary quota window — the week-labeled window when one exists, otherwise the first window. The header summary and reset badge SHALL derive from that same primary window instead of assuming every provider's main window is weekly, so a provider whose main cycle is monthly renders its own label (e.g. `Monthly N%`). An account with no windows at all SHALL still render the no-usage state.

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

#### Scenario: Weekly-only providers render unchanged

Test: UI unit test on the existing weekly fixtures.

- **WHEN** an account carries a week-labeled window
- **THEN** the row headline and reset badge are byte-identical to the previous weekly-only derivation

#### Scenario: Monthly cycle renders its own label

Test: UI unit test with a monthly-only account.

- **WHEN** an account's only window is labeled `Monthly`
- **THEN** the headline reads `Monthly N%` with that window's remaining percentage and reset badge
