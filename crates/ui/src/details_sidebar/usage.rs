use chrono::{DateTime, Utc};
use zeron_proto::{AgentAccountsSnapshot, AgentUsageLine, HarnessId};

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
    pub state: ProviderUsageState,
    pub weekly_summary: Option<String>,
    pub weekly_tone: UsageTone,
    pub windows: Vec<UsageWindowRow>,
    /// Collapsed-header badge for a weekly window about to roll over
    /// (`Reset 12h 16m`) or when remaining quota is exhausted (`Reset 5d 0h`).
    pub weekly_reset_badge: Option<String>,
    pub usage_lines: Vec<AgentUsageLine>,
}

/// Provider mark plus whether the Claude brand tint applies. Kimi reuses the
/// Workers Kimi asset; no duplicate SVG is embedded for Usage.
pub fn usage_provider_icon(harness: HarnessId) -> (&'static str, bool) {
    match harness {
        HarnessId::ClaudeCode => (crate::icons::CLAUDE_MARK, true),
        HarnessId::Kimi => (crate::icons::WORKER_KIMI, false),
        HarnessId::Antigravity => (crate::icons::ANTIGRAVITY, false),
        _ => (crate::icons::OPENAI_MARK, false),
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

pub fn provider_usage_rows(
    snapshot: &AgentAccountsSnapshot,
    now: DateTime<Utc>,
) -> Vec<ProviderUsageRow> {
    [
        (HarnessId::ClaudeCode, "Claude"),
        (HarnessId::Codex, "Codex"),
        (HarnessId::Kimi, "Kimi"),
        (HarnessId::Antigravity, "Antigravity"),
    ]
    .into_iter()
    .map(|(harness, label)| {
        let account = snapshot
            .accounts
            .iter()
            .filter(|account| account.harness == harness)
            .min_by_key(|account| !account.active);
        let Some(account) = account else {
            return ProviderUsageRow {
                harness,
                label,
                account_id: None,
                state: ProviderUsageState::NotSignedIn,
                weekly_summary: None,
                weekly_tone: UsageTone::Neutral,
                windows: Vec::new(),
                weekly_reset_badge: None,
                usage_lines: Vec::new(),
            };
        };
        let windows: Vec<_> = account
            .usage_windows
            .iter()
            .map(|window| {
                let remaining =
                    ((1.0 - window.used_fraction.clamp(0.0, 1.0)) * 100.0).round() as u8;
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
        let weekly_window = windows
            .iter()
            .find(|window| window.label.to_lowercase().contains("week"));
        let weekly_remaining_percent = weekly_window.map(|w| w.remaining_percent);
        let weekly_summary =
            weekly_remaining_percent.map(|remaining| format!("Weekly {remaining}%"));
        let weekly_reset_badge = account
            .usage_windows
            .iter()
            .find(|window| window.label.to_lowercase().contains("week"))
            .and_then(|window| {
                let remaining =
                    ((1.0 - window.used_fraction.clamp(0.0, 1.0)) * 100.0).round() as u8;
                reset_badge_text(window.resets_at, remaining, now)
            });
        let state = if windows.is_empty() && account.usage_lines.is_empty() {
            ProviderUsageState::NoUsage
        } else {
            ProviderUsageState::Ready
        };
        let weekly_tone = if state == ProviderUsageState::Ready {
            weekly_usage_tone(weekly_remaining_percent)
        } else {
            UsageTone::Neutral
        };
        ProviderUsageRow {
            harness,
            label,
            account_id: Some(account.id.clone()),
            state,
            weekly_summary,
            weekly_tone,
            windows,
            weekly_reset_badge,
            usage_lines: account.usage_lines.clone(),
        }
    })
    .collect()
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use zeron_proto::{
        AgentAccount, AgentAccountsSnapshot, AgentUsageLine, AgentUsageWindow, HarnessId,
    };

    use super::{
        ProviderUsageState, UsageTone, derive_usage_pace, provider_usage_rows, reset_text,
        usage_provider_icon,
    };

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
        let rows = provider_usage_rows(&snapshot, now);
        assert_eq!(rows.len(), 4);
        assert_eq!(rows[0].label, "Claude");
        assert_eq!(rows[0].account_id.as_deref(), Some("claude-active"));
        assert_eq!(rows[0].weekly_summary.as_deref(), Some("Weekly 52%"));
        assert_eq!(rows[1].label, "Codex");
        assert_eq!(rows[1].weekly_summary.as_deref(), Some("Weekly 54%"));
        assert_eq!(rows[2].label, "Kimi");
        assert_eq!(rows[2].weekly_summary.as_deref(), Some("Weekly 60%"));
        assert!(rows[2].windows[1].pace.is_some());

        assert_eq!(rows[3].label, "Antigravity");
        assert_eq!(rows[3].account_id.as_deref(), Some("antigravity-managed"));
        assert_eq!(rows[3].weekly_summary.as_deref(), Some("Weekly 67%"));
        assert_eq!(rows[3].weekly_tone, UsageTone::Neutral);
        assert_eq!(rows[3].windows.len(), 4);
        assert_eq!(rows[3].windows[0].label, "Weekly");
        assert_eq!(rows[3].windows[1].label, "5h");
        assert_eq!(rows[3].windows[2].label, "Weekly (Claude/GPT)");
        assert_eq!(rows[3].windows[3].label, "5h (Claude/GPT)");
        assert!(rows[3].windows[0].pace.is_some());
        assert!(rows[3].windows[1].pace.is_some());
        assert!(rows[3].windows[2].pace.is_some());
        assert!(rows[3].windows[3].pace.is_some());

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

        let rows = provider_usage_rows(&snapshot, Utc::now());
        let codex = rows
            .iter()
            .find(|row| row.harness == HarnessId::Codex)
            .unwrap();

        assert_eq!(codex.state, ProviderUsageState::Ready);
        assert_eq!(codex.usage_lines[0].label, "24h");
    }

    #[test]
    fn missing_provider_account_is_explicitly_unavailable() {
        let rows = provider_usage_rows(&AgentAccountsSnapshot::default(), Utc::now());
        assert_eq!(rows.len(), 4);
        assert_eq!(rows[0].state, ProviderUsageState::NotSignedIn);
        assert_eq!(rows[1].state, ProviderUsageState::NotSignedIn);
        assert_eq!(rows[2].state, ProviderUsageState::NotSignedIn);
        assert_eq!(rows[3].state, ProviderUsageState::NotSignedIn);
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
            provider_usage_rows(&snapshot, now)[0]
                .weekly_reset_badge
                .clone()
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
            let rows = provider_usage_rows(&snapshot, now);
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
    fn weekly_tone_neutral_when_not_signed_in_no_usage_or_no_weekly_window() {
        let now = Utc.with_ymd_and_hms(2026, 8, 20, 12, 0, 0).unwrap();
        // NotSignedIn
        let empty_snapshot = AgentAccountsSnapshot::default();
        let rows = provider_usage_rows(&empty_snapshot, now);
        assert_eq!(rows[0].weekly_tone, UsageTone::Neutral);

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
        let rows = provider_usage_rows(&no_usage_snapshot, now);
        assert_eq!(rows[0].weekly_tone, UsageTone::Neutral);

        // Ready but with only non-weekly window (e.g. 5h)
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
        let rows = provider_usage_rows(&non_weekly_snapshot, now);
        let kimi = rows.iter().find(|r| r.harness == HarnessId::Kimi).unwrap();
        assert_eq!(kimi.state, ProviderUsageState::Ready);
        assert_eq!(kimi.weekly_tone, UsageTone::Neutral);
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
        let rows = provider_usage_rows(&make_snapshot(1.0, Some(reset_5d)), now);
        assert_eq!(rows[0].weekly_reset_badge.as_deref(), Some("Reset 5d 0h"));

        // 2. remaining_percent == 60 (comfortable quota) and reset 5 days in the future -> badge absent
        let rows = provider_usage_rows(&make_snapshot(0.40, Some(reset_5d)), now);
        assert_eq!(rows[0].weekly_reset_badge, None);

        // 3. remaining_percent == 60 and reset in 12h (soon) -> badge present
        let rows = provider_usage_rows(&make_snapshot(0.40, Some(reset_12h)), now);
        assert_eq!(rows[0].weekly_reset_badge.as_deref(), Some("Reset 12h 16m"));

        // 4. remaining_percent == 0 and resets_at is None -> badge absent
        let rows = provider_usage_rows(&make_snapshot(1.0, None), now);
        assert_eq!(rows[0].weekly_reset_badge, None);
    }
}
