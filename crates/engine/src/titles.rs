//! Chat auto-titling — after the first user+assistant exchange completes on an
//! untitled chat, name it with the harness's cheapest model (port of zeron's
//! `generateTitle` in `sessions.ts`).
//!
//! Flow (fire-and-forget from the run task; every failure is a silent skip with
//! tracing — a title must never fail or delay a run):
//! 1. skip when the chat already has a title (or has no workspace row);
//! 2. pick the run harness's cheapest model (small-tier name heuristic, else the
//!    last listed model — zeron's `cheapestModel`);
//! 3. run a one-shot, non-streaming-collected titling prompt through the
//!    [`Harness`] trait (read-only sandbox, minimal reasoning, auto-approve),
//!    retrying on zeron's short backoff ladder; fall back to the prompt's first
//!    words when every attempt produces nothing;
//! 4. re-check the title (a user rename during generation wins);
//! 5. when the chat sits in a zeron worktree (`zeron/<name>` branch), rename the
//!    branch from the title and update the chat's branch row;
//! 6. `rename_chat` in the workspace doc.

use std::sync::Arc;

use futures::StreamExt;

use zeron_harness::{CancellationToken, RunControls, SteerMessage};
use zeron_proto::{
    AgentEvent, DoneStatus, HarnessId, Model, ReasoningLevel, RunRequest, SandboxLevel,
    UserInputAnswer, UserInputQuestion,
};

use crate::EngineError;
use crate::registry::HarnessRegistry;
use crate::repos::Repos;
use crate::workspace_host::WorkspaceHost;

/// Throwaway title runs are cheap but still cross a process boundary — retry a
/// couple of times with a short backoff before falling back (zeron's ladder).
const RETRY_DELAYS_MS: &[u64] = &[250, 1_000];

struct Inner {
    workspace: WorkspaceHost,
    registry: Arc<HarnessRegistry>,
    repos: Repos,
}

#[derive(Clone)]
pub struct TitleGenerator {
    inner: Arc<Inner>,
}

impl TitleGenerator {
    pub fn new(workspace: WorkspaceHost, registry: Arc<HarnessRegistry>, repos: Repos) -> Self {
        Self {
            inner: Arc::new(Inner {
                workspace,
                registry,
                repos,
            }),
        }
    }

    /// Fire-and-forget: title `chat_id` if it's still untitled. Called by the run
    /// task after a completed exchange; runs detached so it never delays anything.
    pub fn maybe_generate(&self, chat_id: &str, harness: HarnessId, prompt: &str, cwd: &str) {
        let this = self.clone();
        let chat_id = chat_id.to_string();
        let prompt = prompt.to_string();
        let cwd = cwd.to_string();
        tokio::spawn(async move {
            if let Err(err) = this.generate(&chat_id, harness, &prompt, &cwd).await {
                tracing::debug!(chat = %chat_id, error = %err, "chat auto-titling skipped");
            }
        });
    }

    async fn generate(
        &self,
        chat_id: &str,
        harness_id: HarnessId,
        prompt: &str,
        cwd: &str,
    ) -> Result<(), EngineError> {
        let chat = self
            .inner
            .workspace
            .chat(chat_id)?
            .ok_or_else(|| EngineError::Other("chat has no workspace row".into()))?;
        if chat.title.as_deref().is_some_and(|t| !t.trim().is_empty()) {
            return Ok(()); // already named
        }

        let generated = self.run_title_model(chat_id, harness_id, prompt, cwd).await;
        // Fallback so a chat is always named even if the model run produced nothing.
        let fallback: String = prompt
            .split_whitespace()
            .take(7)
            .collect::<Vec<_>>()
            .join(" ")
            .chars()
            .take(48)
            .collect();
        let title = generated.unwrap_or(fallback);
        if title.is_empty() {
            return Ok(());
        }

        // Re-read after the model call: a user may have named the chat or checked
        // out another branch while the throwaway generation was live.
        let latest = self.inner.workspace.chat(chat_id)?.unwrap_or(chat);
        if latest
            .title
            .as_deref()
            .is_some_and(|t| !t.trim().is_empty())
        {
            return Ok(());
        }

        // Rename the worktree branch when the chat still sits on its original
        // zeron/<name> branch (guards live inside rename_worktree_branch).
        if let (Some(chat_cwd), Some(branch)) = (&latest.cwd, &latest.branch)
            && branch.starts_with("zeron/")
        {
            match self
                .inner
                .repos
                .rename_worktree_branch(std::path::Path::new(chat_cwd), branch, &title)
                .await
            {
                Ok(renamed) if &renamed != branch => {
                    if let Err(err) = self.inner.workspace.set_chat_branch(chat_id, &renamed) {
                        tracing::warn!(chat = %chat_id, error = %err, "chat branch update failed");
                    }
                }
                Ok(_) => {}
                Err(err) => {
                    tracing::warn!(chat = %chat_id, error = %err, "automatic worktree branch rename failed");
                }
            }
        }

        self.inner.workspace.rename_chat(chat_id, &title)?;
        tracing::info!(chat = %chat_id, title = %title, "chat auto-titled");
        Ok(())
    }

    /// One-shot titling run: collect TextDeltas until Done; retries on failure.
    async fn run_title_model(
        &self,
        chat_id: &str,
        harness_id: HarnessId,
        prompt: &str,
        cwd: &str,
    ) -> Option<String> {
        let harness = match self.inner.registry.resolve(harness_id) {
            Ok(harness) => harness,
            Err(err) => {
                tracing::debug!(error = %err, "titling harness unavailable");
                return None;
            }
        };
        let cheap = cheapest_model(&harness.models().await.unwrap_or_default());
        // The title speaks the USER's language, and is capitalized the way that
        // language capitalizes titles. The old prompt asked for "Title Case"
        // with no language rule at all, so an English instruction wrapping a
        // Portuguese request produced an English title — and "Title Case" is an
        // English-only convention anyway (PT/ES/FR capitalize the first word
        // and proper nouns, so imposing it would misspell a correct title).
        let title_prompt = format!(
            "Write a title for a coding session that begins with the request below.\n\n\
             Rules:\n\
             - Reply with ONLY the title: no quotes, no trailing punctuation, no preamble.\n\
             - Between 3 and 5 words.\n\
             - Write it in the SAME LANGUAGE the request is written in.\n\
             - Capitalize it the way titles are capitalized in that language: \
             English uses Title Case; most other languages capitalize only the \
             first word and proper nouns.\n\n\
             Request:\n{prompt}"
        );
        for attempt in 0..=RETRY_DELAYS_MS.len() {
            let request = RunRequest {
                prompt: title_prompt.clone(),
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
            match collect_text(harness.as_ref(), chat_id, request).await {
                Ok(raw) => {
                    let candidate = clean_title(&raw);
                    if !candidate.is_empty() {
                        return Some(candidate);
                    }
                }
                Err(err) => {
                    tracing::warn!(attempt = attempt + 1, error = %err,
                        "automatic chat title generation attempt failed");
                }
            }
            if let Some(delay) = RETRY_DELAYS_MS.get(attempt) {
                tokio::time::sleep(std::time::Duration::from_millis(*delay)).await;
            }
        }
        None
    }
}

/// The cheapest model a harness offers (zeron's `cheapestModel` heuristic):
/// prefer a small-tier name (haiku/mini/nano/flash/small/lite), else the last
/// listed model; `None` when the catalog is empty (harness picks its default).
///
/// Curated catalogs (claude, codex) carry one row per tier, but the OMP harness
/// forwards its runtime's RAW provider inventory — every historical model, in
/// alphabetical order. There a plain first-match picked
/// `anthropic/claude-3-haiku-20240307`, retired at Anthropic, so every titling
/// attempt 404'd and fell back to the prompt's first words. So the first
/// small-tier row only decides the FAMILY (provider prefix + tier word) and the
/// newest member of that family wins. The provider stays pinned: the same
/// family is also listed under providers we may hold no credentials for.
fn cheapest_model(models: &[Model]) -> Option<String> {
    let tier_of = |m: &Model| {
        let haystack = format!("{} {}", m.id, m.label).to_lowercase();
        SMALL_TIERS
            .into_iter()
            .find(move |tier| haystack.contains(tier))
    };
    let small = models.iter().find_map(|first| {
        let tier = tier_of(first)?;
        let provider = model_provider(&first.id);
        models
            .iter()
            .filter(|m| model_provider(&m.id) == provider && tier_of(m) == Some(tier))
            .max_by_key(|m| generation_rank(&m.id))
    });
    small.or(models.last()).map(|m| m.id.clone())
}

const SMALL_TIERS: [&str; 6] = ["haiku", "mini", "nano", "flash", "small", "lite"];

/// OMP ids are `{provider}/{id}` and the inner id may itself hold slashes
/// (`openrouter:pseudo-api/anthropic/claude-3.5-haiku`), so the provider is the
/// first segment. Curated catalog ids have no slash at all.
fn model_provider(id: &str) -> Option<&str> {
    id.split_once('/').map(|(provider, _)| provider)
}

/// Generation order inside one small-tier family: the id's numeric groups with
/// snapshot dates dropped, then the floating alias ranked above its dated pin
/// (`claude-haiku-4-5` over `claude-haiku-4-5-20251001` — same model, stable
/// id). `max_by_key` keeps the LAST maximum, preserving the old "last listed
/// wins" tie-break.
///
/// Both spellings of a snapshot date have to go: packed (`20251001`) and split
/// (`gpt-4o-mini-2024-07-18`). Leaking the split form's components into the
/// version vector ranks `[4, 2024, 7, 18]` above `gpt-4.1-mini`'s `[4, 1]` —
/// the dated pin beating the stable alias, exactly backwards.
fn generation_rank(id: &str) -> (Vec<u32>, bool) {
    let groups: Vec<&str> = id
        .split(|c: char| !c.is_ascii_digit())
        .filter(|g| !g.is_empty())
        .collect();
    let mut version = Vec::new();
    let mut dated = false;
    let mut i = 0;
    while i < groups.len() {
        let group = groups[i];
        if group.len() == 8 && group.starts_with("20") {
            dated = true;
            i += 1;
            continue;
        }
        let split_date = group.len() == 4
            && group.starts_with("20")
            && groups[i + 1..]
                .iter()
                .take(2)
                .filter(|g| g.len() == 2)
                .count()
                == 2;
        if split_date {
            dated = true;
            i += 3;
            continue;
        }
        if let Ok(n) = group.parse::<u32>() {
            version.push(n);
        }
        i += 1;
    }
    (version, !dated)
}

/// First line, stripped of quote/heading dressing, capped at 60 chars.
fn clean_title(raw: &str) -> String {
    let first = raw.trim().lines().next().unwrap_or("");
    first
        .trim_start_matches(['"', '\'', '#', ' ', '\t'])
        .trim_end_matches(['"', '\'', ' ', '\t'])
        .chars()
        .take(60)
        .collect()
}

/// Drive one titling run through the harness: no steering, questions resolved
/// empty immediately (a titling prompt must never block on input).
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
                return Err(EngineError::Other(format!("titling run error: {message}")));
            }
            AgentEvent::Done { status, error, .. } => {
                if status == DoneStatus::Completed {
                    break;
                }
                return Err(EngineError::Other(format!(
                    "titling run ended {status:?}: {}",
                    error.unwrap_or_default()
                )));
            }
            _ => {}
        }
    }
    drop(steer_tx); // keep the mailbox open for the run's whole lifetime
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use zeron_proto::Model;

    fn model(id: &str, label: &str) -> Model {
        Model {
            id: id.into(),
            label: label.into(),
            description: None,
            reasoning_levels: vec![],
            options: vec![],
        }
    }

    #[test]
    fn cheapest_prefers_small_tier_then_last() {
        let models = vec![
            model("opus-4", "Opus"),
            model("haiku-3", "Haiku"),
            model("sonnet-4", "Sonnet"),
        ];
        assert_eq!(cheapest_model(&models).as_deref(), Some("haiku-3"));
        let no_small = vec![model("opus-4", "Opus"), model("sonnet-4", "Sonnet")];
        assert_eq!(cheapest_model(&no_small).as_deref(), Some("sonnet-4"));
        assert_eq!(cheapest_model(&[]), None);
    }

    #[test]
    fn cheapest_picks_the_newest_row_of_the_small_tier_family() {
        // The OMP harness forwards its runtime's raw inventory, alphabetically
        // sorted: retired Haiku 3 sorts ahead of the current Haiku 4.5, and
        // picking it 404'd every titling attempt.
        let models = vec![
            model("anthropic/claude-3-5-sonnet-20241022", "Claude Sonnet 3.5"),
            model("anthropic/claude-3-haiku-20240307", "Claude Haiku 3"),
            model("anthropic/claude-haiku-4-5", "Claude Haiku 4.5"),
            model("anthropic/claude-haiku-4-5-20251001", "Claude Haiku 4.5"),
            model("anthropic/claude-opus-4-6", "Claude Opus 4.6"),
        ];
        assert_eq!(
            cheapest_model(&models).as_deref(),
            Some("anthropic/claude-haiku-4-5")
        );
    }

    #[test]
    fn cheapest_ignores_snapshot_dates_split_by_hyphens() {
        // `2024-07-18` used to leak into the version vector as [4, 2024, 7, 18]
        // and outrank the floating 4.1 alias's [4, 1].
        let models = vec![
            model("openai/gpt-4.1-mini", "GPT-4.1 mini"),
            model("openai/gpt-4o-mini-2024-07-18", "GPT-4o mini"),
        ];
        assert_eq!(
            cheapest_model(&models).as_deref(),
            Some("openai/gpt-4.1-mini")
        );
    }

    #[test]
    fn cheapest_stays_on_the_first_matching_provider() {
        // Same family under three providers: drifting off `anthropic` would
        // pick a route we may hold no credentials for.
        let models = vec![
            model("anthropic/claude-haiku-4-5", "Claude Haiku 4.5"),
            model(
                "google-vertex/claude-haiku-4-5@20251001",
                "Claude Haiku 4.5",
            ),
            model("zenmux/anthropic/claude-haiku-4.5", "Claude Haiku 4.5"),
        ];
        assert_eq!(
            cheapest_model(&models).as_deref(),
            Some("anthropic/claude-haiku-4-5")
        );
    }

    #[test]
    fn titles_are_cleaned() {
        assert_eq!(clean_title("\"Fix Login Flow\"\nextra"), "Fix Login Flow");
        assert_eq!(clean_title("# Add Dark Mode  "), "Add Dark Mode");
        assert_eq!(clean_title("   "), "");
    }
}
