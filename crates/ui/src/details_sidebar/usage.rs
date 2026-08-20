use chrono::{DateTime, Utc};
use zeron_proto::{AgentAccountsSnapshot, HarnessId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderUsageState {
    Ready,
    NoUsage,
    NotSignedIn,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UsageWindowRow {
    pub label: String,
    pub used_fraction: f32,
    pub remaining_percent: u8,
    pub reset_text: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProviderUsageRow {
    pub harness: HarnessId,
    pub label: &'static str,
    pub account_id: Option<String>,
    pub state: ProviderUsageState,
    pub weekly_summary: Option<String>,
    pub windows: Vec<UsageWindowRow>,
}

fn reset_text(resets_at: Option<DateTime<Utc>>, now: DateTime<Utc>) -> Option<String> {
    let resets_at = resets_at?;
    let duration = resets_at.signed_duration_since(now);
    if duration.num_seconds() <= 0 {
        return Some("Reset now".into());
    }
    let days = duration.num_days();
    let hours = duration.num_hours() % 24;
    Some(if days > 0 {
        format!("Reset {days}d {hours}h")
    } else {
        format!(
            "Reset {}h {}m",
            duration.num_hours(),
            duration.num_minutes() % 60
        )
    })
}

pub fn provider_usage_rows(
    snapshot: &AgentAccountsSnapshot,
    now: DateTime<Utc>,
) -> Vec<ProviderUsageRow> {
    [
        (HarnessId::ClaudeCode, "Claude"),
        (HarnessId::Codex, "Codex"),
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
                windows: Vec::new(),
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
                }
            })
            .collect();
        let weekly_summary = windows
            .iter()
            .find(|window| window.label.to_lowercase().contains("week"))
            .map(|window| format!("Weekly {}%", window.remaining_percent));
        ProviderUsageRow {
            harness,
            label,
            account_id: Some(account.id.clone()),
            state: if windows.is_empty() {
                ProviderUsageState::NoUsage
            } else {
                ProviderUsageState::Ready
            },
            weekly_summary,
            windows,
        }
    })
    .collect()
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use zeron_proto::{AgentAccount, AgentAccountsSnapshot, AgentUsageWindow, HarnessId};

    use super::{ProviderUsageState, provider_usage_rows};

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
            display_name: None,
            organization: None,
            auth_kind: None,
            switchable: true,
            saved_at: None,
        }
    }

    #[test]
    fn rows_are_claude_then_codex_and_use_active_accounts() {
        let reset = Utc.with_ymd_and_hms(2026, 8, 25, 12, 0, 0).unwrap();
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
            ],
            warnings: Vec::new(),
        };
        let now = Utc.with_ymd_and_hms(2026, 8, 20, 12, 0, 0).unwrap();
        let rows = provider_usage_rows(&snapshot, now);
        assert_eq!(rows[0].label, "Claude");
        assert_eq!(rows[0].account_id.as_deref(), Some("claude-active"));
        assert_eq!(rows[0].weekly_summary.as_deref(), Some("Weekly 52%"));
        assert_eq!(rows[1].label, "Codex");
        assert_eq!(rows[1].weekly_summary.as_deref(), Some("Weekly 54%"));
    }

    #[test]
    fn missing_provider_account_is_explicitly_unavailable() {
        let rows = provider_usage_rows(&AgentAccountsSnapshot::default(), Utc::now());
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].state, ProviderUsageState::NotSignedIn);
        assert_eq!(rows[1].state, ProviderUsageState::NotSignedIn);
    }
}
