//! Dev/capture knobs: the `ZERON_OPEN_*` / `ZERON_FORCE_*` / `ZERON_DEMO_*`
//! environment variables that boot the viewport straight into a route, dialog,
//! picker, gate or fabricated upload so a screenshot can be taken without
//! synthetic input (headless compositors can't click).
//!
//! They are only honored when `ZERON_UI_CAPTURE` explicitly asks for them. A
//! knob exported once in a shell used to follow every later `cargo run` from
//! that terminal — the app opened on the Accounts settings page for days
//! because `ZERON_OPEN_ROUTE=settings/agents` was still in the environment.
//! One umbrella that a capture session sets on purpose keeps the knobs useful
//! and keeps a stale export from redecorating a normal run.

/// The knob's value, or `None` when this run is not a capture session.
pub(crate) fn knob(name: &str) -> Option<String> {
    knob_with(
        std::env::var("ZERON_UI_CAPTURE").ok().as_deref(),
        std::env::var(name).ok(),
    )
}

/// Split out for tests: no process environment involved.
fn knob_with(umbrella: Option<&str>, value: Option<String>) -> Option<String> {
    matches!(umbrella, Some("1" | "true" | "yes" | "on"))
        .then_some(value)
        .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn knobs_stay_shut_without_the_umbrella() {
        let value = || Some("settings/agents".to_string());
        // The exact case that shipped: a stale export, no capture session.
        assert_eq!(knob_with(None, value()), None);
        assert_eq!(knob_with(Some(""), value()), None);
        assert_eq!(knob_with(Some("0"), value()), None);
        assert_eq!(knob_with(Some("false"), value()), None);
    }

    #[test]
    fn a_capture_session_gets_the_knob() {
        let value = || Some("settings/agents".to_string());
        for umbrella in ["1", "true", "yes", "on"] {
            assert_eq!(
                knob_with(Some(umbrella), value()),
                Some("settings/agents".to_string()),
                "umbrella {umbrella} must open the knobs"
            );
        }
        // Opting in does not invent a value for a knob nobody set.
        assert_eq!(knob_with(Some("1"), None), None);
    }
}
