//! Idle session recap policy and entry types (port of orchestrator.dev idle-recap-policy).
//!
//! Evaluates when an active chat is genuinely idle, determines the timer delay,
//! enforces epoch-based staleness invalidation, and prunes persisted recap entries.

use std::collections::{HashMap, HashSet};
use serde::{Deserialize, Serialize};

/// Minimum seconds of idle before generating a recap.
pub const IDLE_RECAP_MIN_SECONDS: u64 = 60;
/// Maximum seconds of idle before generating a recap.
pub const IDLE_RECAP_MAX_SECONDS: u64 = 600;
/// Default seconds of idle before generating a recap.
pub const IDLE_RECAP_DEFAULT_SECONDS: u64 = 240;

/// Maximum age of a stored recap before it is pruned (24 hours in milliseconds).
pub const IDLE_RECAP_MAX_AGE_MS: u64 = 24 * 60 * 60 * 1000;
/// Bound on the persisted map (newest wins).
pub const IDLE_RECAP_MAX_ENTRIES: usize = 50;

/// One generated recap line, keyed by chat context key in the persisted preferences.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IdleRecapEntry {
    /// The one-line recap text ("where things stand").
    pub text: String,
    /// Transcript message count when generated — the idle epoch this recap covers.
    pub epoch: usize,
    /// Unix timestamp in milliseconds when generated.
    pub generated_at: u64,
}

/// State observed by the idle recap policy.
pub struct IdleRecapState<'a> {
    /// User preference toggle (Settings).
    pub enabled: bool,
    /// Whether generation can proceed (false if previous generation for this epoch failed).
    pub can_generate: bool,
    /// Live turn in progress.
    pub is_streaming: bool,
    /// Context compaction in progress.
    pub is_compacting: bool,
    /// Composer has unsent draft text.
    pub has_draft: bool,
    /// Current transcript message count — the active epoch.
    pub message_count: usize,
    /// Stored recap entry for this chat, if any.
    pub entry: Option<&'a IdleRecapEntry>,
    /// Configured delay in seconds.
    pub delay_seconds: u64,
}

/// Decision returned by [`evaluate_idle_recap`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdleRecapAction {
    /// Nothing to do and nothing stored.
    None,
    /// Drop the stored recap (superseded epoch, live turn, or feature disabled).
    Clear,
    /// A recap for the current epoch exists — show it, do not re-arm.
    Keep,
    /// Idle with no recap for this epoch — arm the timer with clamped milliseconds.
    Arm { delay_ms: u64 },
}

/// Pure policy deciding what the recap lifecycle should do for the current state.
pub fn evaluate_idle_recap(state: &IdleRecapState<'_>) -> IdleRecapAction {
    let has_entry = state.entry.is_some();

    // Feature off or this epoch already failed to generate: never render, never arm.
    if !state.enabled || !state.can_generate {
        return if has_entry {
            IdleRecapAction::Clear
        } else {
            IdleRecapAction::None
        };
    }

    // A turn in flight is replacing the state the line describes.
    if state.is_streaming || state.is_compacting {
        return if has_entry {
            IdleRecapAction::Clear
        } else {
            IdleRecapAction::None
        };
    }

    // Fresh recap for this exact epoch: show it, don't regenerate.
    // Checked BEFORE draft and empty-transcript to survive startup hydration.
    if let Some(entry) = state.entry
        && entry.epoch == state.message_count
    {
        return IdleRecapAction::Keep;
    }

    // No transcript: chat mounted before messages hydrated from storage.
    // Preserves persisted recaps across app launch.
    if state.message_count == 0 {
        return if has_entry {
            IdleRecapAction::Keep
        } else {
            IdleRecapAction::None
        };
    }

    // Draft text: user is actively composing, don't arm.
    // Stored entry for a different epoch is stale and dropped.
    if state.has_draft {
        return if has_entry {
            IdleRecapAction::Clear
        } else {
            IdleRecapAction::None
        };
    }

    let clamped_seconds = state
        .delay_seconds
        .clamp(IDLE_RECAP_MIN_SECONDS, IDLE_RECAP_MAX_SECONDS);
    IdleRecapAction::Arm {
        delay_ms: clamped_seconds * 1000,
    }
}

/// Read/write boundary for the persisted recap map: drops expired and malformed entries,
/// then retains the newest [`IDLE_RECAP_MAX_ENTRIES`].
pub fn prune_idle_recaps(entries: &mut HashMap<String, IdleRecapEntry>, now: u64) {
    // 1. Drop malformed or expired entries (older than 24h or future timestamps)
    entries.retain(|_, entry| {
        !entry.text.trim().is_empty()
            && entry.epoch > 0
            && entry.generated_at <= now
            && now.saturating_sub(entry.generated_at) <= IDLE_RECAP_MAX_AGE_MS
    });

    // 2. Bound to IDLE_RECAP_MAX_ENTRIES newest
    if entries.len() > IDLE_RECAP_MAX_ENTRIES {
        let mut sorted: Vec<(String, u64)> = entries
            .iter()
            .map(|(k, v)| (k.clone(), v.generated_at))
            .collect();
        sorted.sort_by(|a, b| b.1.cmp(&a.1));
        let keys_to_keep: HashSet<String> = sorted
            .into_iter()
            .take(IDLE_RECAP_MAX_ENTRIES)
            .map(|(k, _)| k)
            .collect();
        entries.retain(|k, _| keys_to_keep.contains(k));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_entry(epoch: usize, age_ms: u64, now: u64) -> IdleRecapEntry {
        IdleRecapEntry {
            text: "Reviewed architecture; next: run tests.".to_string(),
            epoch,
            generated_at: now.saturating_sub(age_ms),
        }
    }

    #[test]
    fn test_evaluate_idle_recap_arms_when_idle() {
        let state = IdleRecapState {
            enabled: true,
            can_generate: true,
            is_streaming: false,
            is_compacting: false,
            has_draft: false,
            message_count: 5,
            entry: None,
            delay_seconds: 240,
        };
        assert_eq!(
            evaluate_idle_recap(&state),
            IdleRecapAction::Arm { delay_ms: 240_000 }
        );
    }

    #[test]
    fn test_evaluate_idle_recap_clamps_bounds() {
        let state_low = IdleRecapState {
            enabled: true,
            can_generate: true,
            is_streaming: false,
            is_compacting: false,
            has_draft: false,
            message_count: 5,
            entry: None,
            delay_seconds: 10,
        };
        assert_eq!(
            evaluate_idle_recap(&state_low),
            IdleRecapAction::Arm { delay_ms: 60_000 }
        );

        let state_high = IdleRecapState {
            delay_seconds: 1200,
            ..state_low
        };
        assert_eq!(
            evaluate_idle_recap(&state_high),
            IdleRecapAction::Arm { delay_ms: 600_000 }
        );
    }

    #[test]
    fn test_evaluate_idle_recap_keeps_matching_epoch() {
        let entry = sample_entry(5, 1000, 10_000);
        let state = IdleRecapState {
            enabled: true,
            can_generate: true,
            is_streaming: false,
            is_compacting: false,
            has_draft: false,
            message_count: 5,
            entry: Some(&entry),
            delay_seconds: 240,
        };
        assert_eq!(evaluate_idle_recap(&state), IdleRecapAction::Keep);
    }

    #[test]
    fn test_evaluate_idle_recap_clears_on_streaming_or_draft() {
        let entry = sample_entry(4, 1000, 10_000);
        let state_streaming = IdleRecapState {
            enabled: true,
            can_generate: true,
            is_streaming: true,
            is_compacting: false,
            has_draft: false,
            message_count: 5,
            entry: Some(&entry),
            delay_seconds: 240,
        };
        assert_eq!(evaluate_idle_recap(&state_streaming), IdleRecapAction::Clear);

        let state_draft = IdleRecapState {
            is_streaming: false,
            has_draft: true,
            ..state_streaming
        };
        assert_eq!(evaluate_idle_recap(&state_draft), IdleRecapAction::Clear);
    }

    #[test]
    fn test_evaluate_idle_recap_preserves_on_zero_messages_hydration() {
        let entry = sample_entry(5, 1000, 10_000);
        let state = IdleRecapState {
            enabled: true,
            can_generate: true,
            is_streaming: false,
            is_compacting: false,
            has_draft: false,
            message_count: 0,
            entry: Some(&entry),
            delay_seconds: 240,
        };
        assert_eq!(evaluate_idle_recap(&state), IdleRecapAction::Keep);
    }

    #[test]
    fn test_prune_idle_recaps() {
        let now = 100_000_000;
        let mut map = HashMap::new();

        // Valid recent entry
        map.insert("chat-1".to_string(), sample_entry(2, 3600 * 1000, now));
        // Expired entry (25h old)
        map.insert(
            "chat-2".to_string(),
            sample_entry(2, 25 * 3600 * 1000, now),
        );
        // Future timestamp entry
        map.insert("chat-3".to_string(), IdleRecapEntry {
            text: "Future".to_string(),
            epoch: 1,
            generated_at: now + 5000,
        });
        // Empty text
        map.insert("chat-4".to_string(), IdleRecapEntry {
            text: "   ".to_string(),
            epoch: 1,
            generated_at: now - 1000,
        });

        prune_idle_recaps(&mut map, now);
        assert_eq!(map.len(), 1);
        assert!(map.contains_key("chat-1"));
    }
}
