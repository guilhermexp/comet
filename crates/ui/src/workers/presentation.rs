#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionIndicator {
    Busy,
    Attention,
    Unread,
    Idle,
    Exited,
    Restarting,
}

pub fn session_indicator(
    state: &str,
    activity: &str,
    unread: bool,
    runtime_launch_pending: bool,
) -> SessionIndicator {
    if runtime_launch_pending {
        return SessionIndicator::Restarting;
    }
    if state != "running" {
        return SessionIndicator::Exited;
    }
    match activity {
        "starting" | "working" => SessionIndicator::Busy,
        "blocked" => SessionIndicator::Attention,
        "done" if unread => SessionIndicator::Unread,
        _ if unread => SessionIndicator::Unread,
        _ => SessionIndicator::Idle,
    }
}

pub const SPINNER_FRAMES: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

pub fn spinner_frame(now_unix_ms: u64) -> &'static str {
    SPINNER_FRAMES[((now_unix_ms / 120) as usize) % SPINNER_FRAMES.len()]
}

pub fn relative_age(then_unix_ms: u64, now_unix_ms: u64) -> String {
    let seconds = now_unix_ms.saturating_sub(then_unix_ms) / 1_000;
    match seconds {
        0..=4 => "now".to_owned(),
        5..=59 => format!("{seconds}s"),
        60..=3_599 => format!("{}m", seconds / 60),
        3_600..=86_399 => format!("{}h", seconds / 3_600),
        _ => format!("{}d", seconds / 86_400),
    }
}

#[cfg(test)]
mod tests {
    use super::{SessionIndicator, relative_age, session_indicator, spinner_frame};

    #[test]
    fn running_activity_maps_to_distinct_worker_indicators() {
        assert_eq!(
            session_indicator("running", "starting", false, false),
            SessionIndicator::Busy
        );
        assert_eq!(
            session_indicator("running", "working", false, false),
            SessionIndicator::Busy
        );
        assert_eq!(
            session_indicator("running", "blocked", false, false),
            SessionIndicator::Attention
        );
        assert_eq!(
            session_indicator("running", "done", true, false),
            SessionIndicator::Unread
        );
        assert_eq!(
            session_indicator("exited", "idle", false, false),
            SessionIndicator::Exited
        );
        assert_eq!(
            session_indicator("running", "idle", false, true),
            SessionIndicator::Restarting
        );
        assert_eq!(spinner_frame(0), "⠋");
        assert_eq!(spinner_frame(120), "⠙");
    }

    #[test]
    fn relative_age_is_compact_and_stable() {
        assert_eq!(relative_age(1_000, 1_000), "now");
        assert_eq!(relative_age(1_000, 46_000), "45s");
        assert_eq!(relative_age(1_000, 181_000), "3m");
        assert_eq!(relative_age(1_000, 7_201_000), "2h");
        assert_eq!(relative_age(1_000, 172_801_000), "2d");
    }
}
