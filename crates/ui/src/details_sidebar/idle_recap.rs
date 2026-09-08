//! Idle session recap policy and entry types (port of orchestrator.dev idle-recap-policy).
//!
//! Evaluates when an active chat is genuinely idle, determines the timer delay,
//! enforces epoch-based staleness invalidation, and prunes persisted recap entries.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

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
    if state.is_streaming {
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

    // Compaction is rewriting the transcript under the line: never arm.
    // Unlike streaming this SUPPRESSES without invalidating — `is_compacting`
    // is a sticky flag whose clear path is one session edge away
    // (`shell::finish_compaction`), so letting it drop the stored recap would
    // turn a single missed edge into permanent, silent loss of the line for
    // that chat. Compaction's own transcript marker bumps the epoch, which is
    // what supersedes the entry.
    if state.is_compacting {
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

/// Raw signals read off the live entities, before any policy applies. Keeping
/// the assembly of [`IdleRecapState`] here (rather than inline in the render
/// path) is what makes it testable: there is no gpui render harness.
pub struct IdleRecapSignals {
    /// User preference toggle (Settings).
    pub enabled: bool,
    /// Configured delay in seconds.
    pub delay_seconds: u64,
    /// Live turn in progress for this chat.
    pub is_streaming: bool,
    /// `AppState::is_compacting` for this chat.
    pub is_compacting: bool,
    /// Current transcript message count — the active epoch.
    pub message_count: usize,
    /// Epoch whose previous generation failed, if any.
    pub failed_epoch: Option<usize>,
    /// The composer is showing this chat, so its draft belongs to it.
    pub composer_shows_chat: bool,
    /// `Composer::has_draft` for whatever chat the composer is showing.
    pub composer_has_draft: bool,
}

/// Assembles [`IdleRecapState`] from the raw entity reads and evaluates it.
pub fn evaluate_idle_recap_signals(
    signals: &IdleRecapSignals,
    entry: Option<&IdleRecapEntry>,
) -> IdleRecapAction {
    evaluate_idle_recap(&IdleRecapState {
        enabled: signals.enabled,
        can_generate: signals.failed_epoch != Some(signals.message_count),
        is_streaming: signals.is_streaming,
        is_compacting: signals.is_compacting,
        // `Composer::has_draft` answers for the chat the composer is SHOWING;
        // a sidebar left on another chat must not read that draft as its own.
        has_draft: signals.composer_shows_chat && signals.composer_has_draft,
        message_count: signals.message_count,
        entry,
        delay_seconds: signals.delay_seconds,
    })
}

/// State re-observed when the armed timer fires, right before the recap call
/// spends real model quota: the chat may have moved on while the timer ran.
///
/// Compaction is deliberately absent. `AppState::begin_compaction` is a plain
/// map insert that notifies nothing on its own, but the composer's
/// `state.update` that calls it also pushes the echo message and ends in
/// `cx.notify()`. That notify re-runs [`evaluate_idle_recap`], which returns
/// `Keep`/`None` under `is_compacting` and drops the armed `Task`; a dropped
/// gpui task never fires, so a compaction gate here would be unreachable
/// rather than protective — and the pushed echo has already moved
/// `message_count` past `epoch_at_arm` regardless.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IdleRecapDispatch {
    /// Live turn in progress. Re-checked because the indicator is clock-driven
    /// (a session can change bucket with no state notify to re-evaluate on).
    pub is_streaming: bool,
    /// Transcript message count now.
    pub message_count: usize,
    /// Transcript message count when the timer was armed.
    pub epoch_at_arm: usize,
    /// The armed chat is still the one the sidebar is showing.
    pub is_active_chat: bool,
}

/// Whether the armed timer may actually spend a model turn on the recap.
pub fn should_dispatch_idle_recap(dispatch: &IdleRecapDispatch) -> bool {
    !dispatch.is_streaming
        && dispatch.message_count == dispatch.epoch_at_arm
        && dispatch.is_active_chat
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
        assert_eq!(
            evaluate_idle_recap(&state_streaming),
            IdleRecapAction::Clear
        );

        let state_draft = IdleRecapState {
            is_streaming: false,
            has_draft: true,
            ..state_streaming
        };
        assert_eq!(evaluate_idle_recap(&state_draft), IdleRecapAction::Clear);
    }

    #[test]
    fn test_evaluate_idle_recap_signals_assembly() {
        // Covers the assembly the sidebar used to do inline (draft, compaction
        // and failed-epoch wiring), not just the policy it feeds.
        let idle = IdleRecapSignals {
            enabled: true,
            delay_seconds: 240,
            is_streaming: false,
            is_compacting: false,
            message_count: 5,
            failed_epoch: None,
            composer_shows_chat: true,
            composer_has_draft: false,
        };
        assert_eq!(
            evaluate_idle_recap_signals(&idle, None),
            IdleRecapAction::Arm { delay_ms: 240_000 }
        );

        // Typing in the shown chat is the user not being idle: never arm.
        assert_eq!(
            evaluate_idle_recap_signals(
                &IdleRecapSignals {
                    composer_has_draft: true,
                    ..idle
                },
                None
            ),
            IdleRecapAction::None
        );

        // Same draft, but the composer is on another chat: not this chat's.
        assert_eq!(
            evaluate_idle_recap_signals(
                &IdleRecapSignals {
                    composer_has_draft: true,
                    composer_shows_chat: false,
                    ..idle
                },
                None
            ),
            IdleRecapAction::Arm { delay_ms: 240_000 }
        );

        // Compaction suppresses arming without erasing the stored line.
        let stale = sample_entry(4, 1000, 10_000);
        assert_eq!(
            evaluate_idle_recap_signals(
                &IdleRecapSignals {
                    is_compacting: true,
                    ..idle
                },
                Some(&stale)
            ),
            IdleRecapAction::Keep
        );

        // A generation that already failed for this epoch is not retried.
        assert_eq!(
            evaluate_idle_recap_signals(
                &IdleRecapSignals {
                    failed_epoch: Some(5),
                    ..idle
                },
                None
            ),
            IdleRecapAction::None
        );
    }

    #[test]
    fn test_evaluate_idle_recap_compaction_suppresses_without_erasing() {
        // `is_compacting` is a sticky flag: if a stuck one could reach Clear it
        // would erase the chat's recap and disable the feature there for good.
        // Compaction may only suppress arming.
        let stale = sample_entry(4, 1000, 10_000);
        let compacting = IdleRecapState {
            enabled: true,
            can_generate: true,
            is_streaming: false,
            is_compacting: true,
            has_draft: false,
            message_count: 5,
            entry: Some(&stale),
            delay_seconds: 240,
        };
        assert_eq!(evaluate_idle_recap(&compacting), IdleRecapAction::Keep);
        assert_eq!(
            evaluate_idle_recap(&IdleRecapState {
                entry: None,
                ..compacting
            }),
            IdleRecapAction::None
        );
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
    fn test_should_dispatch_idle_recap() {
        let idle = IdleRecapDispatch {
            is_streaming: false,
            message_count: 5,
            epoch_at_arm: 5,
            is_active_chat: true,
        };
        assert!(should_dispatch_idle_recap(&idle));

        assert!(!should_dispatch_idle_recap(&IdleRecapDispatch {
            is_streaming: true,
            ..idle
        }));
        assert!(!should_dispatch_idle_recap(&IdleRecapDispatch {
            message_count: 6,
            ..idle
        }));
        assert!(!should_dispatch_idle_recap(&IdleRecapDispatch {
            is_active_chat: false,
            ..idle
        }));
    }

    #[test]
    fn test_prune_idle_recaps() {
        let now = 100_000_000;
        let mut map = HashMap::new();

        // Valid recent entry
        map.insert("chat-1".to_string(), sample_entry(2, 3600 * 1000, now));
        // Expired entry (25h old)
        map.insert("chat-2".to_string(), sample_entry(2, 25 * 3600 * 1000, now));
        // Future timestamp entry
        map.insert(
            "chat-3".to_string(),
            IdleRecapEntry {
                text: "Future".to_string(),
                epoch: 1,
                generated_at: now + 5000,
            },
        );
        // Empty text
        map.insert(
            "chat-4".to_string(),
            IdleRecapEntry {
                text: "   ".to_string(),
                epoch: 1,
                generated_at: now - 1000,
            },
        );

        prune_idle_recaps(&mut map, now);
        assert_eq!(map.len(), 1);
        assert!(map.contains_key("chat-1"));
    }
}
