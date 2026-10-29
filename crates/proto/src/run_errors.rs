//! What a run's error means for the person reading it: the provider needs a
//! sign-in, its usage limit was reached, or something else. Both the desktop
//! and the phone read the same error text the harnesses and engine write, and
//! classify it without changing the recorded diagnostic. Desktop presentation
//! offers signing in only for authentication failures, not exhausted quota.

use std::borrow::Cow;

/// A provider-side reason a turn couldn't run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderProblem {
    /// Not signed in, signed out, or a dead/expired login.
    SignedOut,
    /// The account's usage or rate limit was hit; this is not a login failure.
    LimitReached,
}

impl ProviderProblem {
    /// Label for a login action. Callers must gate the action to `SignedOut`.
    pub fn action_label(self) -> &'static str {
        "Log in"
    }
}

const SIGNED_OUT: &[&str] = &[
    "isn't signed in",
    "is not signed in",
    "not signed in",
    "not logged in",
    "please run /login",
    "run `claude login`",
    "run codex login",
    "authentication failed",
    "authentication_error",
    "sign in to claude again",
    "sign in again",
    "log in again",
    "invalid api key",
    "invalid x-api-key",
    "oauth token has expired",
    "token has expired",
    "token expired",
    "refresh token",
    "unauthorized",
    "http 401",
    "status 401",
    "(401)",
    "401 ",
    "logged out",
];

const LIMIT_REACHED: &[&str] = &[
    "usage limit",
    "limit reached",
    "rate limit",
    "rate-limit",
    "you've hit your",
    "you have hit your",
    "hit your limit",
    "quota",
    "http 429",
    "status 429",
    "(429)",
    "too many requests",
];

/// The provider problem `message` reports, if recognized.
pub fn provider_problem(message: &str) -> Option<ProviderProblem> {
    provider_problem_lowercase(&message.to_ascii_lowercase())
}

fn provider_problem_lowercase(text: &str) -> Option<ProviderProblem> {
    // Cloud credits are Zeron's, not the provider's: not a sign-in.
    if text.contains("out of credits") || text.contains("cloud credits") {
        return None;
    }
    // OMP forwards the Anthropic envelope, sometimes with a harness prefix.
    // An explicit rate-limit code must not become signed-out because the
    // diagnostic also happens to mention authentication.
    if text.contains("rate_limit_error") || http_429(text) {
        return Some(ProviderProblem::LimitReached);
    }
    if SIGNED_OUT.iter().any(|needle| text.contains(needle)) {
        return Some(ProviderProblem::SignedOut);
    }
    if LIMIT_REACHED.iter().any(|needle| text.contains(needle)) {
        return Some(ProviderProblem::LimitReached);
    }
    None
}

fn http_429(text: &str) -> bool {
    text.trim_start().starts_with("429 ")
        || text.contains("http 429")
        || text.contains("status 429")
}

/// Human text for desktop error surfaces. Never mutate the stored error: it
/// remains the diagnostic source. Already readable reset explanations and
/// unrecognized failures pass through unchanged.
pub fn provider_error_message(message: &str) -> Cow<'_, str> {
    let text = message.to_ascii_lowercase();
    // The Claude harness already supplies a human headline and may append a
    // readable reset time next to the provider's error code.
    if text.starts_with("claude rate limit reached —") && !text.contains('{') {
        return Cow::Borrowed(message);
    }
    if provider_problem_lowercase(&text) != Some(ProviderProblem::LimitReached)
        || !(text.contains("rate_limit_error")
            || text.contains("retry-after-ms")
            || http_429(&text))
    {
        return Cow::Borrowed(message);
    }
    let Some(ms) = retry_after_ms(&text) else {
        return Cow::Borrowed("Provider limit reached. Try again later.");
    };
    // This is the interval reported at failure time, not a countdown that
    // restarts every time a historical transcript entry is rendered.
    let seconds = ms.div_ceil(1_000);
    let duration = if seconds < 60 {
        format!("{seconds}s")
    } else {
        let minutes = seconds.div_ceil(60);
        match (minutes / 60, minutes % 60) {
            (0, minutes) => format!("{minutes}m"),
            (hours, 0) => format!("{hours}h"),
            (hours, minutes) => format!("{hours}h {minutes}m"),
        }
    };
    Cow::Owned(format!(
        "Provider limit reached. Retry delay reported: {duration}."
    ))
}

fn retry_after_ms(text: &str) -> Option<u64> {
    text.match_indices("retry-after-ms").find_map(|(at, key)| {
        if text[..at]
            .chars()
            .next_back()
            .is_some_and(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
        {
            return None;
        }
        // Accept the OMP suffix and quoted JSON/header fields, including an OMP
        // errorMessage string containing the serialized provider diagnostic.
        let value = text[at + key.len()..].trim_start_matches(['"', '\\', ' ', '\t']);
        let value = value
            .strip_prefix('=')
            .or_else(|| value.strip_prefix(':'))?;
        let value = value.trim_start_matches(['"', '\\', ' ', '\t']);
        let end = value
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(value.len());
        if value[end..].chars().next().is_some_and(|c| {
            !c.is_ascii_whitespace() && !matches!(c, '"' | '\\' | ',' | '}' | ']' | ')')
        }) {
            return None;
        }
        value[..end].parse::<u64>().ok().filter(|ms| *ms > 0)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const ANTHROPIC_LIMIT: &str = r#"429 {"type":"error","error":{"type":"rate_limit_error","message":"This request would exceed your account's rate limit. Please try again later."},"request_id":"req_synthetic"}"#;

    #[test]
    fn provider_limit_presentation_handles_anthropic_and_omp_wrapping() {
        for (delay, expected) in [
            (8_564_000, "2h 23m"),
            (9_239_000, "2h 34m"),
            (9_068_000, "2h 32m"),
            (1, "1s"),
            (60_001, "2m"),
            (3_600_000, "1h"),
        ] {
            let raw = format!("{ANTHROPIC_LIMIT} retry-after-ms={delay}");
            for message in [raw.clone(), format!("harness protocol error: OMP: {raw}")] {
                assert_eq!(
                    provider_problem(&message),
                    Some(ProviderProblem::LimitReached)
                );
                assert_eq!(
                    provider_error_message(&message),
                    format!("Provider limit reached. Retry delay reported: {expected}.")
                );
                assert!(
                    message.contains("req_synthetic"),
                    "source diagnostic is retained"
                );
            }
        }
        let wrapped = serde_json::json!({"errorMessage": format!("{ANTHROPIC_LIMIT} retry-after-ms=8564000")}).to_string();
        assert_eq!(
            provider_error_message(&wrapped),
            "Provider limit reached. Retry delay reported: 2h 23m."
        );
        assert_eq!(
            provider_error_message(
                r#"{"error":{"type":"rate_limit_error"},"headers":{"retry-after-ms":"8564000"}}"#
            ),
            "Provider limit reached. Retry delay reported: 2h 23m."
        );
        assert_eq!(
            provider_error_message(
                "429 rate_limit_error x-retry-after-ms=1000 retry-after-ms=60000"
            ),
            "Provider limit reached. Retry delay reported: 1m."
        );
        // A machine-readable limit takes precedence over incidental auth words.
        assert_eq!(
            provider_problem(
                r#"429 {"type":"rate_limit_error","message":"quota; do not refresh token"}"#
            ),
            Some(ProviderProblem::LimitReached)
        );
    }

    #[test]
    fn provider_limit_presentation_never_invents_a_delay() {
        for suffix in [
            "",
            " retry-after-ms=unknown",
            " retry-after-ms=-1",
            " retry-after-ms=0",
            " retry-after-ms=1.5",
            " retry-after-ms=123oops",
            " retry-after-ms=18446744073709551616",
            " retry-after-ms=",
            " retry-after-ms=1e3",
            " retry-after-ms=+5",
            " x-retry-after-ms=8564000",
        ] {
            assert_eq!(
                provider_error_message(&format!("{ANTHROPIC_LIMIT}{suffix}")),
                "Provider limit reached. Try again later."
            );
        }
        assert_eq!(
            provider_error_message(r#"{"error":{"type":"rate_limit_error"}}"#),
            "Provider limit reached. Try again later."
        );
        assert_eq!(
            provider_error_message("429 Too Many Requests"),
            "Provider limit reached. Try again later."
        );
    }

    #[test]
    fn provider_limit_presentation_preserves_other_diagnostics_and_readable_resets() {
        for message in [
            "Codex usage limit reached — it resets in 2h 14m.",
            "Claude usage limit reached — try again after the limit resets.",
            "Claude rate limit reached — try again after it resets. API Error: 429 rate_limit_error (resets 3pm)",
            "Authentication failed — sign in to Claude again.",
            r#"500 {"type":"api_error","request_id":"req_synthetic"}"#,
            r#"500 {"type":"api_error","message":"failure near rate limit middleware"}"#,
            "Harness exited with status 1",
            "You're out of Cloud credits. Add credits to keep going.",
        ] {
            assert_eq!(provider_error_message(message), message);
        }
        let unrelated = "Authentication failed at line: 429 parsing config";
        assert_eq!(provider_error_message(unrelated), unrelated);
        assert_eq!(
            provider_problem(unrelated),
            Some(ProviderProblem::SignedOut)
        );
        assert_eq!(
            provider_problem("Authentication failed — sign in to Claude again."),
            Some(ProviderProblem::SignedOut)
        );
    }

    #[test]
    fn reads_the_errors_the_harnesses_and_engine_write() {
        for signed_out in [
            "Claude Code isn't signed in on Cloud. Sign in with Claude, pick an account or add an Anthropic API key in Settings → Providers → Cloud, then send again.",
            "Codex isn't signed in on Cloud. Sign in with ChatGPT, pick an account or add an OpenAI API key in Settings → Providers → Cloud, then send again.",
            "Authentication failed — sign in to Claude again.",
            "Invalid API key · Please run /login",
            "OAuth token has expired. Please obtain a new token or refresh your existing token.",
            "Not logged in · Please run /login",
        ] {
            assert_eq!(
                provider_problem(signed_out),
                Some(ProviderProblem::SignedOut),
                "{signed_out}"
            );
        }
        for limited in [
            "Claude usage limit reached — try again after the limit resets.",
            "Claude 5-hour limit reached — the turn was blocked. Try again after it resets.",
            "You've hit your usage limit. Upgrade to Pro or try again later.",
            "429 Too Many Requests",
        ] {
            assert_eq!(
                provider_problem(limited),
                Some(ProviderProblem::LimitReached),
                "{limited}"
            );
        }
        for other in [
            "Claude is overloaded right now — try again shortly.",
            "The selected model isn't available.",
            "You're out of Cloud credits. Add credits to keep going.",
            "Harness exited with status 1",
            "Listening on port 4012",
        ] {
            assert_eq!(provider_problem(other), None, "{other}");
        }
    }
}
