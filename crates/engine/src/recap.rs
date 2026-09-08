//! Idle session recap generation (port of orchestrator.dev recap-generator).
//!
//! Generates a concise, one-line "where things stand" summary of an idle chat
//! from its stored transcript tail without touching the chat's live session,
//! adding turns, or consuming conversation context.

use futures::StreamExt;
use std::sync::Arc;
use std::time::Duration;
use zeron_doc::{MessagePart, MessageRole, SessionMessageEntry};

use crate::EngineError;
use crate::registry::HarnessRegistry;
use zeron_harness::{CancellationToken, RunControls, SteerMessage};
use zeron_proto::{
    AgentEvent, DoneStatus, HarnessId, ReasoningLevel, RunRequest, SandboxLevel, UserInputAnswer,
    UserInputQuestion,
};

/// Max characters of transcript fed to the model.
pub const RECAP_TRANSCRIPT_MAX_CHARS: usize = 6000;
/// Max messages fed to the model, newest-first before reversal.
pub const RECAP_TRANSCRIPT_MAX_MESSAGES: usize = 40;
/// Max characters kept from any single message.
pub const RECAP_ENTRY_MAX_CHARS: usize = 800;
/// Max characters of the final recap line.
pub const RECAP_MAX_CHARS: usize = 280;

/// Default timeout for one-shot recap execution.
const RECAP_TIMEOUT_SECS: u64 = 45;

/// Short retry delays between generation attempts.
const RETRY_DELAYS_MS: &[u64] = &[250, 1_000];

/// A selected transcript entry for recap generation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecapTranscriptEntry {
    pub role: MessageRole,
    pub text: String,
}

/// Slice `text` to at most `max_chars`, trimming at a word boundary when possible.
pub fn slice_at_word(text: &str, max_chars: usize) -> &str {
    if text.len() <= max_chars {
        return text;
    }
    let mut end = 0;
    for (ix, _) in text.char_indices() {
        if ix > max_chars {
            break;
        }
        end = ix;
        if ix == max_chars {
            break;
        }
    }
    if end == 0 {
        return "";
    }
    let candidate = &text[..end];
    if let Some(last_ws) = candidate.rfind(char::is_whitespace) {
        if last_ws > 0 {
            return candidate[..last_ws].trim_end();
        }
    }
    candidate.trim_end()
}

/// Extract plain textual content from a message entry, ignoring tools and non-text parts.
fn entry_text(entry: &SessionMessageEntry) -> String {
    let mut texts = Vec::new();
    for part in &entry.parts {
        if let MessagePart::Text { text, .. } = part {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                texts.push(trimmed);
            }
        }
    }
    texts.join("\n")
}

/// Project the stored messages into a bounded, chronological transcript tail.
/// Walks newest messages backwards so long sessions contribute recent context.
pub fn select_recap_transcript(transcript: &[SessionMessageEntry]) -> Vec<RecapTranscriptEntry> {
    let mut tail = Vec::new();
    let mut budget = RECAP_TRANSCRIPT_MAX_CHARS;

    for entry in transcript.iter().rev() {
        if tail.len() >= RECAP_TRANSCRIPT_MAX_MESSAGES || budget == 0 {
            break;
        }
        if entry.role != MessageRole::User && entry.role != MessageRole::Assistant {
            continue;
        }
        let text = entry_text(entry);
        if text.is_empty() {
            continue;
        }
        let max_len = budget.min(RECAP_ENTRY_MAX_CHARS);
        let clipped = slice_at_word(&text, max_len);
        if clipped.is_empty() {
            break;
        }
        budget = budget.saturating_sub(clipped.len());
        tail.push(RecapTranscriptEntry {
            role: entry.role,
            text: clipped.to_string(),
        });
    }

    tail.reverse();
    tail
}

/// Build the recap prompt matching orchestrator.dev wording and language pin.
pub fn build_recap_prompt(transcript: &[RecapTranscriptEntry], goal: Option<&str>) -> String {
    let conversation = transcript
        .iter()
        .map(|entry| {
            let role_label = match entry.role {
                MessageRole::User => "User",
                MessageRole::Assistant => "Assistant",
                MessageRole::System => "System",
            };
            format!("{}: {}", role_label, entry.text)
        })
        .collect::<Vec<_>>()
        .join("\n\n");

    let anchor = match goal {
        Some(g) if !g.trim().is_empty() => format!("Overall goal: {}\n\n", g.trim()),
        _ => String::new(),
    };

    format!(
        "The user stepped away from this coding session and is coming back. Recap in under 40 words, 1-2 plain sentences, no markdown. Lead with the overall goal and current task, then the one next action. Skip root-cause narrative, fix internals, secondary to-dos, and em-dash tangents. Write in the same language the conversation uses.\n\nReply with ONLY the recap sentence(s).\n\n{}Session so far:\n{}",
        anchor, conversation
    )
}

/// Clean a raw completion into a single-line summary without markdown or preambles.
pub fn validate_recap(raw: Option<&str>) -> Option<String> {
    let raw = raw?.trim();
    if raw.is_empty() {
        return None;
    }

    // Strip code blocks (```...```)
    let mut text = String::with_capacity(raw.len());
    let mut in_fence = false;
    for line in raw.lines() {
        let trimmed_line = line.trim();
        if trimmed_line.starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if !in_fence {
            text.push_str(line);
            text.push(' ');
        }
    }

    // Strip markdown formatting characters
    let stripped: String = text
        .chars()
        .filter(|&c| !matches!(c, '*' | '_' | '`' | '#' | '>'))
        .collect();

    let mut words: Vec<&str> = stripped.split_whitespace().collect();
    if words.is_empty() {
        return None;
    }

    // Strip leading bullet markers
    if matches!(words[0], "-" | "•" | "*") {
        words.remove(0);
    }
    let mut cleaned = words.join(" ");

    // Iteratively strip preambles (case-insensitive)
    for _ in 0..3 {
        let lower = cleaned.to_lowercase();
        let mut stripped_prefix = false;
        for prefix in &[
            "sure:",
            "sure,",
            "sure -",
            "okay:",
            "okay,",
            "okay -",
            "ok:",
            "ok,",
            "ok -",
            "recap:",
            "recap,",
            "recap -",
            "summary:",
            "summary,",
            "summary -",
            "here's the recap:",
            "here is the recap:",
            "here's a recap:",
            "here is a recap:",
            "here's a summary:",
            "here is a summary:",
            "here's what's happening:",
            "here is what's happening:",
        ] {
            if lower.starts_with(prefix) {
                cleaned = cleaned[prefix.len()..].trim().to_string();
                stripped_prefix = true;
                break;
            }
        }
        if !stripped_prefix {
            break;
        }
    }

    let trimmed = cleaned
        .trim_start_matches(['"', '\''])
        .trim_end_matches(['"', '\''])
        .trim();

    if trimmed.is_empty() {
        return None;
    }

    let final_text = slice_at_word(trimmed, RECAP_MAX_CHARS);
    if final_text.is_empty() {
        None
    } else {
        Some(final_text.to_string())
    }
}

async fn collect_text(
    harness: &dyn zeron_harness::Harness,
    chat_id: &str,
    request: RunRequest,
) -> Result<String, EngineError> {
    let (steer_tx, steer_rx) = tokio::sync::mpsc::channel::<SteerMessage>(1);
    let controls = RunControls {
        request_input: Box::new(|_questions: Vec<UserInputQuestion>| {
            let (tx, rx) = tokio::sync::oneshot::channel::<Vec<UserInputAnswer>>();
            let _ = tx.send(Vec::new());
            rx
        }),
        steering: steer_rx,
        interrupt: CancellationToken::new(),
        chat_id: chat_id.to_string(),
    };
    let mut stream = harness.run(request, controls).await?;
    let mut text = String::new();
    while let Some(event) = stream.next().await {
        match event? {
            AgentEvent::TextDelta { text: delta } => text.push_str(&delta),
            AgentEvent::Error { message } => {
                return Err(EngineError::Other(format!("recap run error: {message}")));
            }
            AgentEvent::Done { status, error, .. } => {
                if status == DoneStatus::Completed {
                    break;
                }
                return Err(EngineError::Other(format!(
                    "recap run ended {status:?}: {}",
                    error.unwrap_or_default()
                )));
            }
            _ => {}
        }
    }
    drop(steer_tx);
    Ok(text)
}

/// Run a throwaway one-shot recap generation through the harness.
pub async fn run_recap_model(
    chat_id: &str,
    harness_id: HarnessId,
    prompt: &str,
    cwd: &str,
    registry: &Arc<HarnessRegistry>,
) -> Result<Option<String>, EngineError> {
    let harness = match registry.resolve(harness_id) {
        Ok(h) => h,
        Err(err) => {
            tracing::debug!(error = %err, "recap harness unavailable");
            return Ok(None);
        }
    };
    let cheap = crate::titles::cheapest_model(&harness.models().await.unwrap_or_default());

    for attempt in 0..=RETRY_DELAYS_MS.len() {
        let request = RunRequest {
            prompt: prompt.to_string(),
            harness: Some(harness_id),
            model: cheap.clone(),
            reasoning: Some(ReasoningLevel::Minimal),
            model_options: serde_json::Map::new(),
            cwd: cwd.to_string(),
            sandbox: SandboxLevel::ReadOnly,
            auto_approve: true,
            enable_workers_mcp: false,
            workers_parent_chat_id: None,
            attachments: Vec::new(),
            resume: None,
            worktree: None,
        };

        let run_fut = collect_text(harness.as_ref(), chat_id, request);

        match tokio::time::timeout(Duration::from_secs(RECAP_TIMEOUT_SECS), run_fut).await {
            Ok(Ok(raw)) => {
                if let Some(candidate) = validate_recap(Some(&raw)) {
                    return Ok(Some(candidate));
                }
            }
            Ok(Err(err)) => {
                tracing::warn!(attempt = attempt + 1, error = %err, "recap attempt failed");
            }
            Err(_) => {
                tracing::warn!(attempt = attempt + 1, "recap attempt timed out");
            }
        }

        if let Some(delay) = RETRY_DELAYS_MS.get(attempt) {
            tokio::time::sleep(Duration::from_millis(*delay)).await;
        }
    }

    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text_entry(id: &str, role: MessageRole, text: &str) -> SessionMessageEntry {
        SessionMessageEntry {
            id: id.to_string(),
            role,
            parts: vec![MessagePart::Text {
                id: format!("part-{id}"),
                text: text.to_string(),
            }],
            created_at: 1000,
            device_id: "dev-1".to_string(),
            status: None,
            duration_ms: None,
            continuation_of: None,
        }
    }

    #[test]
    fn test_slice_at_word_bounds() {
        assert_eq!(slice_at_word("hello world", 5), "hello");
        assert_eq!(slice_at_word("hello world", 7), "hello");
        assert_eq!(slice_at_word("hello world", 11), "hello world");
        assert_eq!(slice_at_word("hello world", 15), "hello world");
        assert_eq!(slice_at_word("abcdef", 4), "abcd");
    }

    #[test]
    fn test_select_recap_transcript_bounds() {
        let mut entries = Vec::new();
        for i in 0..100 {
            let role = if i % 2 == 0 {
                MessageRole::User
            } else {
                MessageRole::Assistant
            };
            entries.push(text_entry(
                &format!("msg-{i}"),
                role,
                &format!("message {i} {}", "x".repeat(190)),
            ));
        }

        let tail = select_recap_transcript(&entries);
        assert!(!tail.is_empty());
        assert!(tail.len() <= RECAP_TRANSCRIPT_MAX_MESSAGES);
        assert!(tail.last().unwrap().text.starts_with("message 99"));
        assert!(!tail.iter().any(|e| e.text.starts_with("message 0 ")));
    }

    #[test]
    fn test_select_recap_transcript_drops_non_dialogue() {
        let entries = vec![
            text_entry("1", MessageRole::User, "Hello"),
            text_entry("2", MessageRole::System, "Ignore me"),
            SessionMessageEntry {
                id: "3".to_string(),
                role: MessageRole::Assistant,
                parts: vec![MessagePart::Reasoning {
                    id: "r1".to_string(),
                    text: "thinking...".to_string(),
                    completed: true,
                    duration_ms: None,
                }],
                created_at: 1002,
                device_id: "dev-1".to_string(),
                status: None,
                duration_ms: None,
                continuation_of: None,
            },
            text_entry("4", MessageRole::Assistant, "Task completed"),
        ];

        let tail = select_recap_transcript(&entries);
        assert_eq!(tail.len(), 2);
        assert_eq!(tail[0].text, "Hello");
        assert_eq!(tail[1].text, "Task completed");
    }

    #[test]
    fn test_build_recap_prompt() {
        let entries = vec![
            RecapTranscriptEntry {
                role: MessageRole::User,
                text: "Refactor database queries".to_string(),
            },
            RecapTranscriptEntry {
                role: MessageRole::Assistant,
                text: "Identified N+1 issue in users query".to_string(),
            },
        ];

        let prompt_with_goal = build_recap_prompt(&entries, Some("Performance audit"));
        assert!(prompt_with_goal.contains("Overall goal: Performance audit"));
        assert!(prompt_with_goal.contains("User: Refactor database queries"));
        assert!(prompt_with_goal.contains("Assistant: Identified N+1 issue in users query"));
        assert!(prompt_with_goal.contains("under 40 words"));

        let prompt_no_goal = build_recap_prompt(&entries, None);
        assert!(!prompt_no_goal.contains("Overall goal:"));
    }

    #[test]
    fn test_validate_recap_cleaning() {
        assert_eq!(validate_recap(None), None);
        assert_eq!(validate_recap(Some("   ")), None);

        let clean = validate_recap(Some("Fixed user lookup; next is writing unit tests."));
        assert_eq!(
            clean.as_deref(),
            Some("Fixed user lookup; next is writing unit tests.")
        );

        let formatted = validate_recap(Some(
            "Sure: Here is the recap:\n```rust\ncode\n```\n* Fixed **user** `lookup`; next: tests.",
        ));
        assert_eq!(
            formatted.as_deref(),
            Some("Fixed user lookup; next: tests.")
        );

        let quoted = validate_recap(Some("\"Done with refactoring. Next: commit changes.\""));
        assert_eq!(
            quoted.as_deref(),
            Some("Done with refactoring. Next: commit changes.")
        );
    }
}
