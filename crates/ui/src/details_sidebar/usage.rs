use std::collections::BTreeSet;

use chrono::{DateTime, Utc};
use zeron_proto::{AgentAccount, AgentAccountsSnapshot, AgentUsageLine, HarnessId};

use crate::settings::accounts::{PROVIDERS, provider_accounts};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderUsageState {
    Ready,
    NoUsage,
    NotSignedIn,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsageTone {
    Neutral,
    Warning,
    Danger,
}

pub fn weekly_usage_tone(remaining_percent: Option<u8>) -> UsageTone {
    match remaining_percent {
        Some(0) => UsageTone::Neutral,
        Some(1..=15) => UsageTone::Danger,
        Some(16..=50) => UsageTone::Warning,
        Some(51..) => UsageTone::Neutral,
        None => UsageTone::Neutral,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct UsageWindowRow {
    pub label: String,
    pub used_fraction: f32,
    pub remaining_percent: u8,
    pub reset_text: Option<String>,
    pub pace: Option<UsagePace>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UsagePace {
    pub expected_remaining_fraction: f32,
    pub amount_text: Option<String>,
    pub eta_text: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProviderUsageRow {
    pub harness: HarnessId,
    pub label: &'static str,
    pub account_id: Option<String>,
    pub account_label: Option<String>,
    pub state: ProviderUsageState,
    pub weekly_summary: Option<String>,
    pub weekly_tone: UsageTone,
    pub windows: Vec<UsageWindowRow>,
    /// Collapsed-header badge for a weekly window about to roll over
    /// (`Reset 12h 16m`) or when remaining quota is exhausted (`Reset 5d 0h`).
    pub weekly_reset_badge: Option<String>,
    pub usage_lines: Vec<AgentUsageLine>,
    /// The engine warning for this harness, carried only by rows that have no
    /// quota of their own to show — "No usage yet" names the wrong cause when
    /// the probe failed or the credential expired.
    pub warning: Option<String>,
}

/// Provider mark plus whether the Claude brand tint applies. Kimi reuses the
/// Workers Kimi asset; no duplicate SVG is embedded for Usage.
pub fn usage_provider_icon(harness: HarnessId) -> (&'static str, bool) {
    match harness {
        HarnessId::ClaudeCode => (crate::icons::CLAUDE_MARK, true),
        HarnessId::Kimi => (crate::icons::WORKER_KIMI, false),
        HarnessId::Antigravity => (crate::icons::ANTIGRAVITY, false),
        HarnessId::Cursor => (crate::icons::CURSOR_MARK, false),
        HarnessId::Grok => (crate::icons::GROK_MARK, false),
        _ => (crate::icons::OPENAI_MARK, false),
    }
}

pub fn usage_provider_label(harness: HarnessId) -> &'static str {
    match harness {
        HarnessId::ClaudeCode => "Claude",
        HarnessId::Codex => "Codex",
        HarnessId::Kimi => "Kimi",
        HarnessId::Antigravity => "Antigravity",
        HarnessId::Cursor => "Cursor",
        HarnessId::Grok => "Grok",
        _ => "Agent",
    }
}

/// How close a weekly reset has to be for the collapsed header to display a
/// countdown badge when quota remains. When quota is exhausted, the countdown
/// badge is always displayed as long as a future reset timestamp exists.
pub const RESET_SOON_HOURS: i64 = 48;

fn reset_text(resets_at: Option<DateTime<Utc>>, now: DateTime<Utc>) -> Option<String> {
    let resets_at = resets_at?;
    let duration = resets_at.signed_duration_since(now);
    if duration.num_seconds() <= 0 {
        return Some("Resets soon".into());
    }
    let days = duration.num_days();
    let hours = duration.num_hours() % 24;
    let minutes = duration.num_minutes() % 60;
    Some(if days > 0 {
        format!("Resets in {days}d {hours}h")
    } else if duration.num_hours() > 0 {
        format!("Resets in {}h {minutes}m", duration.num_hours())
    } else {
        format!("Resets in {minutes}m")
    })
}

/// [`reset_text`] restated as a badge (`Reset 12h 16m` or `Reset 5d 0h`) while the
/// window is inside [`RESET_SOON_HOURS`] or when remaining quota is 0%. Reusing one
/// formatter keeps the badge and the expanded row from ever disagreeing about the same countdown.
fn reset_badge_text(
    resets_at: Option<DateTime<Utc>>,
    remaining_percent: u8,
    now: DateTime<Utc>,
) -> Option<String> {
    let resets_at = resets_at?;
    let left = resets_at.signed_duration_since(now);
    if left.num_seconds() <= 0 {
        return None;
    }
    if left > chrono::Duration::hours(RESET_SOON_HOURS) && remaining_percent != 0 {
        return None;
    }
    Some(reset_text(Some(resets_at), now)?.replacen("Resets in ", "Reset ", 1))
}

fn compact_duration(duration: chrono::Duration) -> Option<String> {
    if duration.num_seconds() <= 0 {
        return None;
    }
    let days = duration.num_days();
    let hours = duration.num_hours() % 24;
    let minutes = duration.num_minutes() % 60;
    Some(if days > 0 {
        format!("{days}d {hours}h")
    } else if duration.num_hours() > 0 {
        format!("{}h {minutes}m", duration.num_hours())
    } else if duration.num_minutes() > 0 {
        format!("{}m", duration.num_minutes())
    } else {
        "<1m".into()
    })
}

pub fn derive_usage_pace(
    used_fraction: f32,
    resets_at: Option<DateTime<Utc>>,
    window_duration_mins: Option<i64>,
    now: DateTime<Utc>,
) -> Option<UsagePace> {
    let resets_at = resets_at?;
    let duration_mins = window_duration_mins?;
    if duration_mins <= 0 || now >= resets_at {
        return None;
    }
    let duration = chrono::Duration::minutes(duration_mins);
    let elapsed = now.signed_duration_since(resets_at - duration);
    if elapsed.num_milliseconds() <= 0 {
        return None;
    }
    let elapsed_fraction =
        ((elapsed.num_milliseconds() as f64 / duration.num_milliseconds() as f64).max(0.05)) as f32;
    let used = used_fraction.clamp(0.0, 1.0);
    let expected_used = elapsed_fraction.clamp(0.0, 1.0);
    let expected_remaining = 1.0 - expected_used;
    let delta = used - expected_used;
    let rounded = (delta.abs() * 100.0).round() as u32;
    let amount_text = (rounded > 0).then(|| {
        if delta > 0.0 {
            format!("{rounded}% in deficit")
        } else {
            format!("{rounded}% in reserve")
        }
    });
    let projected_used = if used == 0.0 {
        0.0
    } else {
        used / elapsed_fraction
    };
    let behind = used >= 1.0 || projected_used > 1.0;
    let eta_text = if !behind {
        Some("Lasts until reset".into())
    } else if used >= 1.0 {
        Some("Limit reached".into())
    } else {
        let remaining_window = resets_at.signed_duration_since(now);
        let eta_ms = ((1.0 - used) / projected_used * duration.num_milliseconds() as f32) as i64;
        let eta = chrono::Duration::milliseconds(eta_ms);
        (eta < remaining_window)
            .then(|| compact_duration(eta).map(|text| format!("Runs out in {text}")))
            .flatten()
    };
    Some(UsagePace {
        expected_remaining_fraction: expected_remaining,
        amount_text,
        eta_text,
    })
}

/// One row per visible account. The two ways a provider ends up with none are
/// NOT the same thing and do not render the same:
///
/// - **No account detected.** The credentials behind this widget come from the
///   device (Keychain, `~/.codex`, `~/.kimi`, `~/.cli-proxy-api`, `~/.cursor`,
///   `~/.grok`), not from anything Comet did, so a provider silently dropping
///   off reads as Comet losing it. It stays, as a `NotSignedIn` placeholder.
/// - **Every account hidden.** That is the user working the Accounts toggle.
///   An explicit opt-out removes the provider; leaving a placeholder behind
///   would make the toggle look broken.
pub fn provider_usage_rows(
    snapshot: &AgentAccountsSnapshot,
    hidden_account_ids: &BTreeSet<String>,
    now: DateTime<Utc>,
) -> Vec<ProviderUsageRow> {
    PROVIDERS
        .into_iter()
        .flat_map(|(harness, _, _)| {
            let warning = snapshot
                .warnings
                .iter()
                .find(|warning| warning.harness == harness)
                .map(|warning| warning.message.clone());
            let detected = provider_accounts(snapshot, harness);
            if detected.is_empty() {
                return vec![placeholder_usage_row(harness, warning)];
            }
            let visible: Vec<&AgentAccount> = detected
                .into_iter()
                .filter(|account| !hidden_account_ids.contains(&account.id))
                .collect();
            let show_account_label = visible.len() > 1;
            visible
                .into_iter()
                .map(|account| account_usage_row(account, show_account_label, warning.clone(), now))
                .collect::<Vec<_>>()
        })
        .collect()
}

/// A provider with no account detected on this device.
fn placeholder_usage_row(harness: HarnessId, warning: Option<String>) -> ProviderUsageRow {
    ProviderUsageRow {
        harness,
        label: usage_provider_label(harness),
        account_id: None,
        account_label: None,
        state: ProviderUsageState::NotSignedIn,
        weekly_summary: None,
        weekly_tone: UsageTone::Neutral,
        windows: Vec::new(),
        weekly_reset_badge: None,
        usage_lines: Vec::new(),
        warning,
    }
}

fn account_usage_row(
    account: &AgentAccount,
    show_account_label: bool,
    warning: Option<String>,
    now: DateTime<Utc>,
) -> ProviderUsageRow {
    let windows: Vec<_> = account
        .usage_windows
        .iter()
        .map(|window| {
            let remaining = ((1.0 - window.used_fraction.clamp(0.0, 1.0)) * 100.0).round() as u8;
            UsageWindowRow {
                label: window.label.clone(),
                used_fraction: window.used_fraction.clamp(0.0, 1.0),
                remaining_percent: remaining,
                reset_text: reset_text(window.resets_at, now),
                pace: derive_usage_pace(
                    window.used_fraction,
                    window.resets_at,
                    if window.label.to_lowercase().contains("week") {
                        Some(10_080)
                    } else if window.label.to_lowercase().contains("session")
                        || window.label.to_lowercase().contains("5h")
                    {
                        Some(300)
                    } else {
                        None
                    },
                    now,
                ),
            }
        })
        .collect();
    // Primary quota window: week-labeled when present, otherwise the first window.
    // This allows non-weekly cycles (Cursor's billing-cycle Monthly) to render
    // their own headline ("Monthly 37%") instead of blanking out as "—".
    let primary_window = windows
        .iter()
        .find(|window| window.label.to_lowercase().contains("week"))
        .or_else(|| windows.first());
    let primary_remaining_percent = primary_window.map(|w| w.remaining_percent);
    let weekly_summary =
        primary_window.map(|window| format!("{} {}%", window.label, window.remaining_percent));
    let weekly_reset_badge = primary_window.and_then(|window| {
        reset_badge_text(
            account
                .usage_windows
                .iter()
                .find(|w| w.label == window.label)
                .and_then(|w| w.resets_at),
            window.remaining_percent,
            now,
        )
    });
    let state = if windows.is_empty() && account.usage_lines.is_empty() {
        ProviderUsageState::NoUsage
    } else {
        ProviderUsageState::Ready
    };
    let weekly_tone = if state == ProviderUsageState::Ready {
        weekly_usage_tone(primary_remaining_percent)
    } else {
        UsageTone::Neutral
    };
    ProviderUsageRow {
        harness: account.harness,
        label: usage_provider_label(account.harness),
        account_id: Some(account.id.clone()),
        account_label: show_account_label
            .then(|| {
                account
                    .email
                    .clone()
                    .or_else(|| account.display_name.clone())
            })
            .flatten(),
        state,
        weekly_summary,
        weekly_tone,
        windows,
        weekly_reset_badge,
        usage_lines: account.usage_lines.clone(),
        // A provider already showing quota must not double as an error channel.
        warning: (state != ProviderUsageState::Ready)
            .then_some(warning)
            .flatten(),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use chrono::{DateTime, TimeZone, Utc};
    use zeron_proto::{
        AgentAccount, AgentAccountWarning, AgentAccountsSnapshot, AgentUsageLine, AgentUsageWindow,
        HarnessId,
    };

    use super::{
        PROVIDERS, ProviderUsageRow, ProviderUsageState, UsageTone, derive_usage_pace,
        provider_usage_rows, reset_text, usage_provider_icon,
    };

    fn usage_rows(snapshot: &AgentAccountsSnapshot, now: DateTime<Utc>) -> Vec<ProviderUsageRow> {
        provider_usage_rows(snapshot, &BTreeSet::new(), now)
    }

    fn usage_rows_hiding(
        snapshot: &AgentAccountsSnapshot,
        hidden: &[&str],
        now: DateTime<Utc>,
    ) -> Vec<ProviderUsageRow> {
        let hidden = hidden.iter().map(|id| (*id).to_string()).collect();
        provider_usage_rows(snapshot, &hidden, now)
    }

    /// Rows that carry a real account, i.e. the list minus the placeholders every
    /// provider without a visible login contributes.
    fn account_rows(rows: &[ProviderUsageRow]) -> Vec<&ProviderUsageRow> {
        rows.iter().filter(|row| row.account_id.is_some()).collect()
    }

    fn rows_for(rows: &[ProviderUsageRow], harness: HarnessId) -> Vec<&ProviderUsageRow> {
        rows.iter().filter(|row| row.harness == harness).collect()
    }

    #[test]
    fn weekly_pace_reports_deficit_and_projected_exhaustion() {
        let now = Utc.with_ymd_and_hms(2026, 8, 20, 12, 0, 0).unwrap();
        let reset = Utc.with_ymd_and_hms(2026, 8, 25, 12, 0, 0).unwrap();
        let pace = derive_usage_pace(0.67, Some(reset), Some(10_080), now).unwrap();
        assert!(pace.expected_remaining_fraction > 0.70);
        assert!(
            pace.amount_text
                .as_deref()
                .is_some_and(|text| text.contains("deficit"))
        );
        assert!(
            pace.eta_text
                .as_deref()
                .is_some_and(|text| text.starts_with("Runs out in"))
        );
    }

    fn account(
        id: &str,
        harness: HarnessId,
        active: bool,
        windows: Vec<AgentUsageWindow>,
    ) -> AgentAccount {
        AgentAccount {
            id: id.into(),
            harness,
            email: None,
            plan_label: None,
            active,
            usage_windows: windows,
            usage_lines: vec![],
            display_name: None,
            organization: None,
            auth_kind: None,
            switchable: true,
            saved_at: None,
        }
    }

    #[test]
    fn monthly_window_renders_its_own_headline_summary() {
        let now = Utc.with_ymd_and_hms(2026, 9, 7, 12, 0, 0).unwrap();
        let reset = Utc.with_ymd_and_hms(2026, 10, 7, 12, 0, 0).unwrap();
        let snapshot = AgentAccountsSnapshot {
            accounts: vec![account(
                "cursor-active",
                HarnessId::Cursor,
                true,
                vec![AgentUsageWindow {
                    label: "Monthly".into(),
                    used_fraction: 0.63,
                    resets_at: Some(reset),
                }],
            )],
            warnings: vec![],
        };
        let rows = usage_rows(&snapshot, now);
        let cursor_row = rows
            .iter()
            .find(|row| row.harness == HarnessId::Cursor)
            .expect("cursor row");
        assert_eq!(cursor_row.weekly_summary.as_deref(), Some("Monthly 37%"));
        assert_eq!(cursor_row.state, ProviderUsageState::Ready);
        assert_eq!(cursor_row.weekly_tone, UsageTone::Warning);
    }

    #[test]
    fn rows_are_claude_then_codex_then_kimi_then_antigravity_and_use_active_accounts() {
        let reset = Utc.with_ymd_and_hms(2026, 8, 25, 12, 0, 0).unwrap();
        let rolling_reset = Utc.with_ymd_and_hms(2026, 8, 20, 15, 0, 0).unwrap();
        let snapshot = AgentAccountsSnapshot {
            accounts: vec![
                account(
                    "codex-active",
                    HarnessId::Codex,
                    true,
                    vec![AgentUsageWindow {
                        label: "Weekly".into(),
                        used_fraction: 0.46,
                        resets_at: Some(reset),
                    }],
                ),
                account(
                    "claude-old",
                    HarnessId::ClaudeCode,
                    false,
                    vec![AgentUsageWindow {
                        label: "Weekly".into(),
                        used_fraction: 0.99,
                        resets_at: Some(reset),
                    }],
                ),
                account(
                    "claude-active",
                    HarnessId::ClaudeCode,
                    true,
                    vec![AgentUsageWindow {
                        label: "Weekly".into(),
                        used_fraction: 0.48,
                        resets_at: Some(reset),
                    }],
                ),
                account(
                    "kimi-managed",
                    HarnessId::Kimi,
                    true,
                    vec![
                        AgentUsageWindow {
                            label: "Weekly".into(),
                            used_fraction: 0.40,
                            resets_at: Some(reset),
                        },
                        AgentUsageWindow {
                            label: "5h".into(),
                            used_fraction: 0.25,
                            resets_at: Some(rolling_reset),
                        },
                    ],
                ),
                account(
                    "antigravity-managed",
                    HarnessId::Antigravity,
                    true,
                    vec![
                        AgentUsageWindow {
                            label: "Weekly".into(),
                            used_fraction: 0.3279389,
                            resets_at: Some(reset),
                        },
                        AgentUsageWindow {
                            label: "5h".into(),
                            used_fraction: 0.0,
                            resets_at: Some(rolling_reset),
                        },
                        AgentUsageWindow {
                            label: "Weekly (Claude/GPT)".into(),
                            used_fraction: 0.0,
                            resets_at: Some(reset),
                        },
                        AgentUsageWindow {
                            label: "5h (Claude/GPT)".into(),
                            used_fraction: 0.0,
                            resets_at: Some(rolling_reset),
                        },
                    ],
                ),
            ],
            warnings: Vec::new(),
        };
        let now = Utc.with_ymd_and_hms(2026, 8, 20, 12, 0, 0).unwrap();
        let rows = usage_rows_hiding(&snapshot, &["claude-old"], now);
        // Every provider is listed; Cursor and Grok have no login here and ride
        // along as placeholders after the four detected ones.
        assert_eq!(rows.len(), PROVIDERS.len());
        let detected = account_rows(&rows);
        assert_eq!(detected.len(), 4);
        assert_eq!(detected[0].label, "Claude");
        assert_eq!(detected[0].account_id.as_deref(), Some("claude-active"));
        assert_eq!(detected[0].weekly_summary.as_deref(), Some("Weekly 52%"));
        assert_eq!(detected[1].label, "Codex");
        assert_eq!(detected[1].weekly_summary.as_deref(), Some("Weekly 54%"));
        assert_eq!(detected[2].label, "Kimi");
        assert_eq!(detected[2].weekly_summary.as_deref(), Some("Weekly 60%"));
        assert!(detected[2].windows[1].pace.is_some());

        assert_eq!(detected[3].label, "Antigravity");
        assert_eq!(
            detected[3].account_id.as_deref(),
            Some("antigravity-managed")
        );
        assert_eq!(detected[3].weekly_summary.as_deref(), Some("Weekly 67%"));
        assert_eq!(detected[3].weekly_tone, UsageTone::Neutral);
        assert_eq!(detected[3].windows.len(), 4);
        assert_eq!(detected[3].windows[0].label, "Weekly");
        assert_eq!(detected[3].windows[1].label, "5h");
        assert_eq!(detected[3].windows[2].label, "Weekly (Claude/GPT)");
        assert_eq!(detected[3].windows[3].label, "5h (Claude/GPT)");
        assert!(detected[3].windows[0].pace.is_some());
        assert!(detected[3].windows[1].pace.is_some());
        assert!(detected[3].windows[2].pace.is_some());
        assert!(detected[3].windows[3].pace.is_some());
        assert_eq!(
            rows.iter().map(|row| row.label).collect::<Vec<_>>(),
            ["Claude", "Codex", "Kimi", "Antigravity", "Cursor", "Grok"]
        );

        assert_eq!(
            usage_provider_icon(HarnessId::Antigravity),
            (crate::icons::ANTIGRAVITY, false)
        );
        assert_eq!(
            usage_provider_icon(HarnessId::Kimi),
            (crate::icons::WORKER_KIMI, false)
        );
    }

    #[test]
    fn local_usage_lines_keep_provider_ready_without_remote_windows() {
        let mut codex = account("codex-local", HarnessId::Codex, true, vec![]);
        codex.usage_lines = vec![AgentUsageLine {
            label: "24h".into(),
            value: "12K tokens".into(),
            subtitle: Some("2 recent sessions".into()),
        }];
        let snapshot = AgentAccountsSnapshot {
            accounts: vec![codex],
            warnings: Vec::new(),
        };

        let rows = usage_rows(&snapshot, Utc::now());
        let codex = rows
            .iter()
            .find(|row| row.harness == HarnessId::Codex)
            .unwrap();

        assert_eq!(codex.state, ProviderUsageState::Ready);
        assert_eq!(codex.usage_lines[0].label, "24h");
    }

    #[test]
    fn empty_snapshot_still_lists_every_provider() {
        let rows = usage_rows(&AgentAccountsSnapshot::default(), Utc::now());
        assert_eq!(
            rows.iter().map(|row| row.label).collect::<Vec<_>>(),
            ["Claude", "Codex", "Kimi", "Antigravity", "Cursor", "Grok"]
        );
        assert!(rows.iter().all(|row| {
            row.account_id.is_none()
                && row.state == ProviderUsageState::NotSignedIn
                && row.weekly_tone == UsageTone::Neutral
                && row.weekly_summary.is_none()
        }));
    }

    #[test]
    fn detected_providers_sit_beside_not_signed_in_ones() {
        let now = Utc::now();
        let snapshot = AgentAccountsSnapshot {
            accounts: vec![account(
                "claude-active",
                HarnessId::ClaudeCode,
                true,
                vec![AgentUsageWindow {
                    label: "Weekly".into(),
                    used_fraction: 0.25,
                    resets_at: None,
                }],
            )],
            warnings: vec![],
        };

        let rows = usage_rows(&snapshot, now);
        assert_eq!(rows.len(), PROVIDERS.len());
        assert_eq!(rows[0].account_id.as_deref(), Some("claude-active"));
        assert_eq!(rows[0].weekly_summary.as_deref(), Some("Weekly 75%"));
        assert!(
            rows[1..]
                .iter()
                .all(|row| row.state == ProviderUsageState::NotSignedIn)
        );
    }

    #[test]
    fn warning_replaces_the_generic_summary_only_on_rows_without_quota() {
        let now = Utc::now();
        let mut codex = account("codex-active", HarnessId::Codex, true, vec![]);
        codex.usage_windows = vec![AgentUsageWindow {
            label: "Weekly".into(),
            used_fraction: 0.15,
            resets_at: None,
        }];
        let snapshot = AgentAccountsSnapshot {
            accounts: vec![
                account("kimi-managed", HarnessId::Kimi, true, vec![]),
                codex,
            ],
            warnings: vec![
                AgentAccountWarning {
                    harness: HarnessId::Kimi,
                    message: "Kimi Code Usage returned an invalid payload".into(),
                },
                AgentAccountWarning {
                    harness: HarnessId::Codex,
                    message: "ignored while quota is showing".into(),
                },
                AgentAccountWarning {
                    harness: HarnessId::Grok,
                    message: "No Grok subscription detected on this device".into(),
                },
            ],
        };

        let rows = usage_rows(&snapshot, now);
        let kimi = rows_for(&rows, HarnessId::Kimi)[0];
        assert_eq!(kimi.state, ProviderUsageState::NoUsage);
        assert_eq!(
            kimi.warning.as_deref(),
            Some("Kimi Code Usage returned an invalid payload")
        );
        // A provider already showing quota is not an error channel.
        assert_eq!(rows_for(&rows, HarnessId::Codex)[0].warning, None);
        // The placeholder carries its harness warning too.
        assert_eq!(
            rows_for(&rows, HarnessId::Grok)[0].warning.as_deref(),
            Some("No Grok subscription detected on this device")
        );
    }

    #[test]
    fn weekly_reset_badge_only_inside_the_emphasis_window() {
        let now = Utc.with_ymd_and_hms(2026, 8, 20, 12, 0, 0).unwrap();
        let badge = |reset| {
            let snapshot = AgentAccountsSnapshot {
                accounts: vec![account(
                    "claude-active",
                    HarnessId::ClaudeCode,
                    true,
                    vec![AgentUsageWindow {
                        label: "Weekly".into(),
                        used_fraction: 0.10,
                        resets_at: Some(reset),
                    }],
                )],
                warnings: Vec::new(),
            };
            usage_rows(&snapshot, now)[0].weekly_reset_badge.clone()
        };

        // Emphasis tracks RESET PROXIMITY, never how much quota is left: this
        // window sits at 10% used and still earns the badge.
        assert_eq!(
            badge(now + chrono::Duration::minutes(12 * 60 + 16)).as_deref(),
            Some("Reset 12h 16m")
        );
        assert_eq!(badge(now + chrono::Duration::hours(72)), None);
        assert_eq!(badge(now - chrono::Duration::minutes(1)), None);
    }

    #[test]
    fn reset_countdown_drops_an_empty_hours_segment() {
        let now = Utc.with_ymd_and_hms(2026, 8, 20, 12, 0, 0).unwrap();
        assert_eq!(
            reset_text(Some(now + chrono::Duration::minutes(45)), now).as_deref(),
            Some("Resets in 45m")
        );
        assert_eq!(
            reset_text(Some(now + chrono::Duration::minutes(12 * 60 + 16)), now).as_deref(),
            Some("Resets in 12h 16m")
        );
    }

    #[test]
    fn weekly_tone_boundaries() {
        let now = Utc.with_ymd_and_hms(2026, 8, 20, 12, 0, 0).unwrap();
        let check_tone = |used_fraction| {
            let snapshot = AgentAccountsSnapshot {
                accounts: vec![account(
                    "claude-active",
                    HarnessId::ClaudeCode,
                    true,
                    vec![AgentUsageWindow {
                        label: "Weekly".into(),
                        used_fraction,
                        resets_at: None,
                    }],
                )],
                warnings: Vec::new(),
            };
            let rows = usage_rows(&snapshot, now);
            rows[0].weekly_tone
        };

        // remaining_percent = round((1 - used) * 100)
        // 0.49 -> 51% -> Neutral
        assert_eq!(check_tone(0.49), UsageTone::Neutral);
        // 0.50 -> 50% -> Warning
        assert_eq!(check_tone(0.50), UsageTone::Warning);
        // 0.84 -> 16% -> Warning
        assert_eq!(check_tone(0.84), UsageTone::Warning);
        // 0.85 -> 15% -> Danger
        assert_eq!(check_tone(0.85), UsageTone::Danger);
        // 0.99 -> 1% -> Danger
        assert_eq!(check_tone(0.99), UsageTone::Danger);
        // 1.00 -> 0% -> Neutral
        assert_eq!(check_tone(1.00), UsageTone::Neutral);
    }

    #[test]
    fn primary_window_summary_and_tone_stay_paired() {
        let now = Utc.with_ymd_and_hms(2026, 8, 20, 12, 0, 0).unwrap();
        // NoUsage
        let no_usage_snapshot = AgentAccountsSnapshot {
            accounts: vec![account(
                "claude-active",
                HarnessId::ClaudeCode,
                true,
                vec![],
            )],
            warnings: Vec::new(),
        };
        // NoUsage invents neither summary nor tone.
        let no_usage_rows = usage_rows(&no_usage_snapshot, now);
        assert_eq!(no_usage_rows[0].weekly_summary, None);
        assert_eq!(no_usage_rows[0].weekly_tone, UsageTone::Neutral);

        // Only a non-weekly window (e.g. 5h): it becomes the primary window, so it
        // drives summary and tone together — the widget paints the summary with the tone.
        let non_weekly_snapshot = AgentAccountsSnapshot {
            accounts: vec![account(
                "kimi-active",
                HarnessId::Kimi,
                true,
                vec![AgentUsageWindow {
                    label: "5h".into(),
                    used_fraction: 0.90,
                    resets_at: None,
                }],
            )],
            warnings: Vec::new(),
        };
        let rows = usage_rows(&non_weekly_snapshot, now);
        let kimi = rows.iter().find(|r| r.harness == HarnessId::Kimi).unwrap();
        assert_eq!(kimi.state, ProviderUsageState::Ready);
        assert_eq!(kimi.weekly_summary.as_deref(), Some("5h 10%"));
        assert_eq!(kimi.weekly_tone, UsageTone::Danger);
    }

    #[test]
    fn weekly_reset_badge_shows_when_exhausted_even_with_distant_reset() {
        let now = Utc.with_ymd_and_hms(2026, 8, 20, 12, 0, 0).unwrap();
        let reset_5d = now + chrono::Duration::days(5);
        let reset_12h = now + chrono::Duration::minutes(12 * 60 + 16);

        let make_snapshot = |used_fraction, resets_at| AgentAccountsSnapshot {
            accounts: vec![account(
                "claude-active",
                HarnessId::ClaudeCode,
                true,
                vec![AgentUsageWindow {
                    label: "Weekly".into(),
                    used_fraction,
                    resets_at,
                }],
            )],
            warnings: Vec::new(),
        };

        // 1. remaining_percent == 0 and reset 5 days in the future -> badge present (e.g. "Reset 5d 0h")
        let rows = usage_rows(&make_snapshot(1.0, Some(reset_5d)), now);
        assert_eq!(rows[0].weekly_reset_badge.as_deref(), Some("Reset 5d 0h"));

        // 2. remaining_percent == 60 (comfortable quota) and reset 5 days in the future -> badge absent
        let exhausted_distant = usage_rows(&make_snapshot(0.40, Some(reset_5d)), now);
        assert_eq!(exhausted_distant[0].weekly_reset_badge, None);

        // 3. remaining_percent == 60 and reset in 12h (soon) -> badge present
        let soon = usage_rows(&make_snapshot(0.40, Some(reset_12h)), now);
        assert_eq!(soon[0].weekly_reset_badge.as_deref(), Some("Reset 12h 16m"));

        // 4. remaining_percent == 0 and resets_at is None -> badge absent
        let no_reset = usage_rows(&make_snapshot(1.0, None), now);
        assert_eq!(no_reset[0].weekly_reset_badge, None);
    }

    #[test]
    fn hidden_account_is_omitted_and_two_visible_claudes_are_two_rows() {
        let now = Utc::now();
        let mut alice = account("claude-alice", HarnessId::ClaudeCode, true, vec![]);
        alice.email = Some("alice@example.com".into());
        let mut bob = account("claude-bob", HarnessId::ClaudeCode, false, vec![]);
        bob.email = Some("bob@example.com".into());
        let snapshot = AgentAccountsSnapshot {
            accounts: vec![alice, bob],
            warnings: vec![],
        };

        let rows = usage_rows(&snapshot, now);
        let visible = rows_for(&rows, HarnessId::ClaudeCode);
        assert_eq!(visible.len(), 2);
        assert_eq!(visible[0].account_id.as_deref(), Some("claude-alice"));
        assert_eq!(
            visible[0].account_label.as_deref(),
            Some("alice@example.com")
        );
        assert_eq!(visible[1].account_id.as_deref(), Some("claude-bob"));
        assert_eq!(visible[1].account_label.as_deref(), Some("bob@example.com"));

        let rows = usage_rows_hiding(&snapshot, &["claude-bob"], now);
        let hidden = rows_for(&rows, HarnessId::ClaudeCode);
        assert_eq!(hidden.len(), 1);
        assert_eq!(hidden[0].account_id.as_deref(), Some("claude-alice"));
        assert_eq!(hidden[0].account_label, None);
    }

    #[test]
    fn cursor_account_appears_when_visible() {
        let snapshot = AgentAccountsSnapshot {
            accounts: vec![account("cursor-1", HarnessId::Cursor, true, vec![])],
            warnings: vec![],
        };
        let rows = usage_rows(&snapshot, Utc::now());
        let cursor = rows_for(&rows, HarnessId::Cursor);
        assert_eq!(cursor.len(), 1);
        assert_eq!(cursor[0].account_id.as_deref(), Some("cursor-1"));
        assert_eq!(
            usage_provider_icon(HarnessId::Cursor),
            (crate::icons::CURSOR_MARK, false)
        );
    }

    #[test]
    fn grok_account_appears_when_visible() {
        let snapshot = AgentAccountsSnapshot {
            accounts: vec![account("grok-1", HarnessId::Grok, true, vec![])],
            warnings: vec![],
        };
        let rows = usage_rows(&snapshot, Utc::now());
        let grok = rows_for(&rows, HarnessId::Grok);
        assert_eq!(grok.len(), 1);
        assert_eq!(grok[0].account_id.as_deref(), Some("grok-1"));
        assert_eq!(
            usage_provider_icon(HarnessId::Grok),
            (crate::icons::GROK_MARK, false)
        );
    }

    #[test]
    fn hiding_every_account_of_a_provider_removes_it_entirely() {
        let snapshot = AgentAccountsSnapshot {
            accounts: vec![account("kimi-managed", HarnessId::Kimi, true, vec![])],
            warnings: vec![AgentAccountWarning {
                harness: HarnessId::Kimi,
                message: "Kimi Code Usage returned an invalid payload".into(),
            }],
        };

        // Detected and visible: its own row, no placeholder beside it.
        let shown = usage_rows(&snapshot, Utc::now());
        assert_eq!(rows_for(&shown, HarnessId::Kimi).len(), 1);
        assert_eq!(
            rows_for(&shown, HarnessId::Kimi)[0].account_id.as_deref(),
            Some("kimi-managed")
        );

        // Toggled off: the provider leaves the widget. A placeholder here would
        // make the Accounts toggle look like it did nothing — and the warning
        // goes with it, since the user asked not to see this provider.
        let hidden = usage_rows_hiding(&snapshot, &["kimi-managed"], Utc::now());
        assert!(rows_for(&hidden, HarnessId::Kimi).is_empty());
        assert_eq!(hidden.len(), PROVIDERS.len() - 1);
    }

    #[test]
    fn hiding_one_of_two_accounts_keeps_the_provider_without_a_placeholder() {
        let now = Utc::now();
        let mut alice = account("claude-alice", HarnessId::ClaudeCode, true, vec![]);
        alice.email = Some("alice@example.com".into());
        let mut bob = account("claude-bob", HarnessId::ClaudeCode, false, vec![]);
        bob.email = Some("bob@example.com".into());
        let snapshot = AgentAccountsSnapshot {
            accounts: vec![alice, bob],
            warnings: vec![],
        };

        let rows = usage_rows_hiding(&snapshot, &["claude-bob"], now);
        let claude = rows_for(&rows, HarnessId::ClaudeCode);
        assert_eq!(claude.len(), 1);
        assert_eq!(claude[0].account_id.as_deref(), Some("claude-alice"));
        assert_ne!(claude[0].state, ProviderUsageState::NotSignedIn);
    }
}
