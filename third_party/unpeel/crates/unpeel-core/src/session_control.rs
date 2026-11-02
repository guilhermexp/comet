//! Shared Worker-control operations behind the peer `sessions` tool
//! (`mcp_host`) and Comet's controller `workers` tool: target resolution and
//! listing, terminal output reads, transcript reads with their two profiles,
//! the wait vocabulary and verdict, key encoding, terminal output cleanup and
//! the one input delivery path.
//!
//! This module holds no policy and no lifecycle verbs. Each adapter runs its
//! own authorization first (the peer's self/group/approval rules, the
//! controller's authority marker and parent/episode checks) and then calls
//! in here with an explicit [`InputAuthority`], read profile or
//! [`WaitVocabulary`]. Only the controller variant can attach a task receipt
//! or wait for task completion; it labels the call path, it is not a
//! security boundary by itself (everything runs as the same user). The JSON
//! shape each adapter answers with stays its own.

use crate::session_host::{self, HostedSessionManifest, HostedSessionState};

/// At most this many key entries per call, for peers and the controller.
pub const MAX_KEYS_PER_CALL: usize = 64;
/// At most this many bytes in one `text:<literal>` key entry.
pub const MAX_TEXT_KEY_BYTES: usize = 16 * 1024;

/// Who is writing, after that adapter's own authorization passed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputAuthority {
    /// A Worker acting on a peer through `sessions`.
    Peer,
    /// Comet's controller acting on a Worker it supervises. `task_episode`
    /// records a parent-Chat task receipt on the submitting Enter.
    Controller { task_episode: Option<u64> },
}

/// Terminal bytes for one key name: named keys (`enter`, `tab`, `shift+tab`,
/// `escape`, arrows, `home`, `end`, `page_up`, `page_down`, `backspace`,
/// `delete`, `space`), `ctrl+x` / `ctrl-x`, `text:<literal>` (case kept, at
/// most [`MAX_TEXT_KEY_BYTES`]) or one printable character.
pub fn key_sequence(key: &str) -> Result<String, String> {
    if let Some(literal) = key.strip_prefix("text:") {
        if literal.len() > MAX_TEXT_KEY_BYTES {
            return Err("one text: key entry exceeds 16 KiB".into());
        }
        return Ok(literal.to_string());
    }
    let normalized = key.trim().to_ascii_lowercase();
    let named = match normalized.as_str() {
        "enter" | "return" => Some("\r"),
        "tab" => Some("\t"),
        "shift+tab" | "shift-tab" | "backtab" => Some("\x1b[Z"),
        "space" => Some(" "),
        "esc" | "escape" => Some("\x1b"),
        "up" => Some("\x1b[A"),
        "down" => Some("\x1b[B"),
        "right" => Some("\x1b[C"),
        "left" => Some("\x1b[D"),
        "home" => Some("\x1b[H"),
        "end" => Some("\x1b[F"),
        "pageup" | "page_up" => Some("\x1b[5~"),
        "pagedown" | "page_down" => Some("\x1b[6~"),
        "backspace" => Some("\x7f"),
        "delete" | "del" => Some("\x1b[3~"),
        _ => None,
    };
    if let Some(sequence) = named {
        return Ok(sequence.to_string());
    }
    if let Some(rest) = normalized
        .strip_prefix("ctrl+")
        .or_else(|| normalized.strip_prefix("ctrl-"))
    {
        let mut chars = rest.chars();
        if let (Some(c), None) = (chars.next(), chars.next()) {
            if c.is_ascii_lowercase() {
                return Ok(((c as u8 - b'a' + 1) as char).to_string());
            }
        }
        return Err(format!("Unsupported key: {key:?}"));
    }
    let mut chars = key.chars();
    if let (Some(c), None) = (chars.next(), chars.next()) {
        if !c.is_control() {
            return Ok(c.to_string());
        }
    }
    Err(format!("Unsupported key: {key:?}"))
}

/// Every entry encoded, or an error before anything could be written.
pub fn encode_keys<S: AsRef<str>>(keys: &[S]) -> Result<Vec<String>, String> {
    if keys.is_empty() {
        return Err("send_keys requires at least one key".into());
    }
    if keys.len() > MAX_KEYS_PER_CALL {
        return Err(format!(
            "send_keys accepts at most {MAX_KEYS_PER_CALL} keys per call"
        ));
    }
    keys.iter().map(|key| key_sequence(key.as_ref())).collect()
}

/// How a cleaned read treats a bare carriage return.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CarriageReturns {
    /// The peer's readable text: every carriage return becomes a line break.
    AsLineBreaks,
    /// The controller's text: a carriage return inside a line is kept, so a
    /// consumer can tell a repainted status line (`10%\r50%`) from new lines
    /// and keep only the last paint. CRLF still ends a line as LF.
    Keep,
}

/// Terminal output as readable text: escape sequences (CSI, OSC and the
/// ST-terminated DCS/APC/SOS/PM strings) removed, carriage returns as line
/// breaks, other control characters dropped and long blank runs collapsed.
pub fn strip_ansi(text: &str) -> String {
    clean_terminal_text(text, CarriageReturns::AsLineBreaks)
}

/// The one terminal-text cleaner behind [`strip_ansi`]: escapes removed,
/// control characters other than newline, tab and (per `carriage_returns`)
/// carriage return dropped, CRLF ended as LF and long blank runs collapsed.
pub fn clean_terminal_text(text: &str, carriage_returns: CarriageReturns) -> String {
    enum State {
        Ground,
        Escape,
        EscapeIntermediate,
        Csi,
        Osc,
        OscEscape,
    }

    let mut state = State::Ground;
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match state {
            State::Ground => match c {
                '\u{1b}' => state = State::Escape,
                '\r' => {
                    if chars.peek() == Some(&'\n') {
                        chars.next();
                        out.push('\n');
                    } else {
                        out.push(match carriage_returns {
                            CarriageReturns::AsLineBreaks => '\n',
                            CarriageReturns::Keep => '\r',
                        });
                    }
                }
                c if !c.is_control() || matches!(c, '\n' | '\t') => out.push(c),
                _ => {}
            },
            State::Escape => match c {
                '[' => state = State::Csi,
                ']' | 'P' | '_' | 'X' | '^' => state = State::Osc,
                '\u{20}'..='\u{2f}' => state = State::EscapeIntermediate,
                _ => state = State::Ground,
            },
            State::EscapeIntermediate => {
                if !('\u{20}'..='\u{2f}').contains(&c) {
                    state = State::Ground;
                }
            }
            State::Csi => {
                if ('\u{40}'..='\u{7e}').contains(&c) {
                    state = State::Ground;
                }
            }
            State::Osc => match c {
                '\u{07}' => state = State::Ground,
                '\u{1b}' => state = State::OscEscape,
                _ => {}
            },
            State::OscEscape => {
                state = if c == '\\' { State::Ground } else { State::Osc };
            }
        }
    }

    let mut collapsed = String::with_capacity(out.len());
    let mut blank_run = 0usize;
    for line in out.lines() {
        if line.trim().is_empty() {
            blank_run += 1;
            if blank_run > 2 {
                continue;
            }
        } else {
            blank_run = 0;
        }
        collapsed.push_str(line);
        collapsed.push('\n');
    }
    collapsed
}

/// Why a target cannot take input. Both are known before any byte is sent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TargetError {
    Unknown,
    Exited,
}

impl TargetError {
    pub fn message(&self, session_id: &str) -> String {
        match self {
            Self::Unknown => format!("Unknown session id '{session_id}'."),
            Self::Exited => format!("Session '{session_id}' has exited and cannot receive input."),
        }
    }
}

/// Whether a session id can name a session directory at all.
pub fn safe_session_id(session_id: &str) -> bool {
    !session_id.is_empty()
        && !session_id.contains('/')
        && !session_id.contains("..")
        && !session_id.contains('\\')
}

/// The target's manifest in any state (path-checked id). Reads only durable
/// state; never pings the Host.
pub fn read_target(session_id: &str) -> Result<HostedSessionManifest, TargetError> {
    if !safe_session_id(session_id) {
        return Err(TargetError::Unknown);
    }
    session_host::load_manifest(session_id).ok_or(TargetError::Unknown)
}

/// Which hosted sessions an enumeration returns, before adapter policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionFilter {
    /// Every manifest on this Host (the controller's bootstrap).
    All,
    /// Only sessions whose Host is running (the peer's list).
    Running,
}

/// Hosted sessions on this Host, newest first. Each adapter applies its own
/// reach/authority filter on top.
pub fn hosted_sessions(filter: SessionFilter) -> Vec<HostedSessionManifest> {
    let mut manifests = session_host::list_manifests();
    if filter == SessionFilter::Running {
        manifests.retain(|manifest| manifest.state == HostedSessionState::Running);
    }
    manifests.sort_by_key(|manifest| std::cmp::Reverse(manifest.session.created_at));
    manifests
}

/// The target's manifest when it is a running hosted session (path-checked
/// id). Reads only durable state; never pings the Host.
pub fn live_target(session_id: &str) -> Result<HostedSessionManifest, TargetError> {
    let manifest = read_target(session_id)?;
    if manifest.state != HostedSessionState::Running {
        return Err(TargetError::Exited);
    }
    Ok(manifest)
}

/// Raw key bytes into a live target through the one input choke point.
pub fn send_keys(session_id: &str, encoded: &str, authority: InputAuthority) -> Result<(), String> {
    let _ = authority;
    live_target(session_id).map_err(|error| error.message(session_id))?;
    crate::session_ops::write_session_input(session_id, encoded.to_string(), None, None, None)
}

/// Already-sanitized text through the bracketed-paste prompt recipe. Only
/// the controller may attach a task receipt (always submitted).
pub fn send_text(
    session_id: &str,
    sanitized: &str,
    submit: bool,
    authority: InputAuthority,
) -> Result<(), String> {
    live_target(session_id).map_err(|error| error.message(session_id))?;
    match authority {
        InputAuthority::Controller {
            task_episode: Some(episode),
        } => crate::session_input::deliver_sanitized_text_with_task_receipt(
            session_id, sanitized, episode,
        ),
        InputAuthority::Peer | InputAuthority::Controller { task_episode: None } => {
            crate::session_input::deliver_sanitized_text(session_id, sanitized, submit)
        }
    }
}

/// Where a terminal output read starts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputCursor {
    /// The most recent `max` bytes (the peer's low-context read).
    Tail { max: usize },
    /// Forward from `offset` (the end of the journal's tail when `None`),
    /// at most `max` bytes (the controller's paged read).
    From { offset: Option<u64>, max: usize },
}

/// One terminal output read: raw bytes plus the journal offsets they span.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutputRead {
    pub data: Vec<u8>,
    /// Lifetime offset of `data[0]`.
    pub offset: u64,
    pub next_offset: u64,
    /// Older bytes exist before `offset`, or the requested offset had aged
    /// out and the read began at the earliest retained byte instead.
    pub truncated: bool,
    pub exited: bool,
}

/// Read a hosted session's terminal output journal (path-checked id).
pub fn read_output(session_id: &str, cursor: OutputCursor) -> Result<OutputRead, String> {
    if !safe_session_id(session_id) {
        return Err(TargetError::Unknown.message(session_id));
    }
    let (offset, max, tail) = match cursor {
        OutputCursor::Tail { max } => (None, max.max(1), Some(max.max(1))),
        OutputCursor::From { offset, max } => (offset, max.max(1), Some(max.max(1))),
    };
    let chunk = session_host::read_output_chunk(session_id, offset, Some(max), tail)?;
    let start = chunk.next_offset.saturating_sub(chunk.data.len() as u64);
    Ok(OutputRead {
        truncated: offset.map_or(start > 0, |requested| requested != start),
        offset: start,
        next_offset: chunk.next_offset,
        exited: chunk.exited,
        data: chunk.data,
    })
}

/// How much of a provider transcript a read returns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TranscriptProfile {
    /// The peer's low-context tail: Settings → Transcripts defaults with
    /// the entry count capped at `max_entries` and an optional tools
    /// override.
    Compact {
        entries: Option<usize>,
        max_entries: usize,
        default_entries: usize,
        include_tools: Option<bool>,
    },
    /// The controller's whole-message export: Settings → Transcripts range
    /// unless overridden (`0` is the whole conversation); callers page it
    /// with [`page_text`].
    Whole { entries: Option<usize> },
}

/// Render a session's provider transcript as Markdown under one profile.
pub fn read_transcript(session_id: &str, profile: TranscriptProfile) -> Result<String, String> {
    let manifest = read_target(session_id).map_err(|error| error.message(session_id))?;
    match profile {
        TranscriptProfile::Whole { entries } => {
            crate::transcripts::read_session_transcript_markdown(
                &manifest.session.id,
                entries,
                false,
            )
        }
        TranscriptProfile::Compact {
            entries,
            max_entries,
            default_entries,
            include_tools,
        } => {
            let mut opts = crate::transcripts::load_transcript_settings();
            if let Some(entries) = entries {
                opts.max_entries = entries.clamp(1, max_entries);
            } else if opts.max_entries == 0 || opts.max_entries > max_entries {
                opts.max_entries = default_entries;
            }
            if let Some(include_tools) = include_tools {
                opts.include_tools = include_tools;
            }
            let collect_tools = opts.include_tools
                || opts.include_reasoning
                || opts.include_file_changes
                || opts.include_plan_updates;
            let snapshot = crate::transcripts::read_transcript_snapshot(
                &manifest,
                opts.max_entries,
                collect_tools,
                None,
            )?;
            if snapshot.entries.is_empty() {
                return Err(format!(
                    "Found {} transcript at {}, but no readable user/assistant entries were found.",
                    snapshot.provider, snapshot.path
                ));
            }
            let body = crate::transcripts::format_transcript_markdown(&snapshot, &opts);
            Ok(format!(
                "transcript provider={} source={} session={} path={}\n{}\n{}",
                snapshot.provider,
                snapshot.source,
                snapshot.provider_session_id.as_deref().unwrap_or("unknown"),
                snapshot.path,
                "-".repeat(40),
                body.trim_end()
            ))
        }
    }
}

/// One byte page of rendered text, snapped to char boundaries.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextPage<'a> {
    pub text: &'a str,
    pub start: usize,
    pub end: usize,
    pub total: usize,
    pub next_offset: Option<usize>,
    pub previous_offset: Option<usize>,
}

impl TextPage<'_> {
    pub fn truncated(&self) -> bool {
        self.start > 0 || self.end < self.total
    }
}

/// A page of `text`: without `offset` the tail (most recent content), with
/// it the page starting at that byte. Offsets are what `next_offset` and
/// `previous_offset` hand back, so a caller walks the whole text.
pub fn page_text(text: &str, offset: Option<u64>, page_bytes: usize) -> TextPage<'_> {
    let total = text.len();
    let boundary = |mut index: usize, forward: bool| {
        index = index.min(total);
        while !text.is_char_boundary(index) {
            index = if forward { index + 1 } else { index - 1 };
        }
        index
    };
    let (start, end) = match offset {
        Some(offset) => {
            let start = boundary(usize::try_from(offset).unwrap_or(usize::MAX), false);
            (start, boundary(start.saturating_add(page_bytes), false))
        }
        None => (boundary(total.saturating_sub(page_bytes), true), total),
    };
    TextPage {
        text: &text[start..end],
        start,
        end,
        total,
        next_offset: (end < total).then_some(end),
        // Rounded forward to a char boundary: a floored offset would make
        // the previous page end before this one starts and drop a character.
        previous_offset: (start > 0).then(|| boundary(start.saturating_sub(page_bytes), true)),
    }
}

/// Which wait statuses an adapter accepts and how it compares them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WaitVocabulary {
    /// The peer's activity labels; `done` and `idle` are one settled state.
    /// A peer has no task authority, so it cannot wait for completion.
    Peer,
    /// The controller's activity and lifecycle labels plus `completed`,
    /// which only current-episode evidence satisfies.
    Controller,
}

impl WaitVocabulary {
    pub const PEER_STATUSES: &'static [&'static str] = &[
        "starting", "working", "blocked", "done", "idle", "exited", "unknown",
    ];
    pub const CONTROLLER_STATUSES: &'static [&'static str] = &[
        "completed",
        "running",
        "exited",
        "starting",
        "working",
        "blocked",
        "done",
        "idle",
    ];

    pub fn statuses(self) -> &'static [&'static str] {
        match self {
            Self::Peer => Self::PEER_STATUSES,
            Self::Controller => Self::CONTROLLER_STATUSES,
        }
    }
}

/// A validated wait status in one vocabulary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WaitTarget {
    status: String,
    vocabulary: WaitVocabulary,
}

impl WaitTarget {
    pub fn status(&self) -> &str {
        &self.status
    }

    pub fn wants_completion(&self) -> bool {
        self.status == "completed"
    }
}

/// Parse a wait status in `vocabulary` (case-insensitive for the controller,
/// exact for peers, as each tool has always accepted).
pub fn parse_wait_target(status: &str, vocabulary: WaitVocabulary) -> Result<WaitTarget, String> {
    let normalized = match vocabulary {
        WaitVocabulary::Peer => status.to_owned(),
        WaitVocabulary::Controller => status.to_ascii_lowercase(),
    };
    if vocabulary.statuses().contains(&normalized.as_str()) {
        return Ok(WaitTarget {
            status: normalized,
            vocabulary,
        });
    }
    Err(match vocabulary {
        WaitVocabulary::Peer => format!(
            "Unsupported status {status:?}; expected one of {}",
            WaitVocabulary::PEER_STATUSES.join(", ")
        ),
        WaitVocabulary::Controller => format!(
            "Unknown status '{status}'. Accepted values are: {}",
            WaitVocabulary::CONTROLLER_STATUSES.join(", ")
        ),
    })
}

/// What one poll observed about the target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WaitObservation<'a> {
    /// Activity label (`working`, `idle`, `blocked`, …).
    pub activity: &'a str,
    /// Lifecycle label (`running`, `exited`, …).
    pub state: &'a str,
    /// The current task episode has completion evidence (controller only).
    pub task_completed: bool,
    /// Completion evidence was already present at the first poll.
    pub completed_at_start: bool,
    /// The current turn was interrupted (Escape) before finishing.
    pub interrupted: bool,
}

/// The outcome of judging one observation against a wait target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WaitVerdict {
    Matched,
    /// Blocked on input while waiting for a terminal outcome: not
    /// completion, nothing was approved.
    NeedsInput,
    /// The awaited task episode was interrupted and can never complete.
    Interrupted,
    /// A lifecycle wait saw a new task completion instead.
    EpisodeCompleted,
    Pending,
}

/// Judge one poll. `completed` is never satisfied by `idle` or `exited`:
/// only task evidence (or an explicit `completed` label) matches it.
pub fn judge_wait(target: &WaitTarget, observation: WaitObservation<'_>) -> WaitVerdict {
    let wanted = target.status.as_str();
    match target.vocabulary {
        WaitVocabulary::Peer => {
            let current = observation.activity;
            let settled = matches!((current, wanted), ("done", "idle") | ("idle", "done"));
            if current == wanted || settled {
                WaitVerdict::Matched
            } else {
                WaitVerdict::Pending
            }
        }
        WaitVocabulary::Controller => {
            let label_matched = observation.activity.eq_ignore_ascii_case(wanted)
                || observation.state.eq_ignore_ascii_case(wanted);
            if label_matched {
                return WaitVerdict::Matched;
            }
            let blocked = observation.activity.eq_ignore_ascii_case("blocked");
            if target.wants_completion() {
                if blocked {
                    return WaitVerdict::NeedsInput;
                }
                if observation.task_completed {
                    return WaitVerdict::Matched;
                }
                if !observation.activity.eq_ignore_ascii_case("working") && observation.interrupted
                {
                    return WaitVerdict::Interrupted;
                }
                return WaitVerdict::Pending;
            }
            if wanted == "exited" && blocked {
                return WaitVerdict::NeedsInput;
            }
            if !observation.completed_at_start && observation.task_completed {
                return WaitVerdict::EpisodeCompleted;
            }
            WaitVerdict::Pending
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peer_and_controller_spellings_encode_identically() {
        for (a, b) in [
            ("ctrl+c", "ctrl-c"),
            ("shift+tab", "shift-tab"),
            ("esc", "escape"),
            ("pageup", "page_up"),
        ] {
            assert_eq!(key_sequence(a), key_sequence(b), "{a} vs {b}");
        }
        assert_eq!(key_sequence("ctrl+r").unwrap(), "\x12");
        assert_eq!(key_sequence("Y").unwrap(), "Y");
        assert_eq!(key_sequence("text:Hello World").unwrap(), "Hello World");
        assert_eq!(key_sequence("down").unwrap(), "\x1b[B");
        assert!(key_sequence("ctrl+shift+x").is_err());
        assert!(key_sequence("madeup").is_err());
        assert!(key_sequence(&format!("text:{}", "x".repeat(MAX_TEXT_KEY_BYTES + 1))).is_err());
    }

    #[test]
    fn encoding_is_all_or_nothing_and_bounded() {
        assert_eq!(
            encode_keys(&["escape", "down", "enter"]).unwrap().concat(),
            "\u{1b}\u{1b}[B\r"
        );
        assert!(encode_keys(&["enter", "unknown-special"]).is_err());
        assert!(encode_keys::<&str>(&[]).is_err());
        assert!(encode_keys(&vec!["enter"; MAX_KEYS_PER_CALL + 1]).is_err());
        assert_eq!(
            encode_keys(&vec!["enter"; MAX_KEYS_PER_CALL])
                .unwrap()
                .len(),
            MAX_KEYS_PER_CALL
        );
    }

    #[test]
    fn missing_or_hostile_targets_are_rejected_before_any_write() {
        for id in ["", "../x", "a/b", "a\\b", "no-such-session-for-tests"] {
            assert_eq!(live_target(id).unwrap_err(), TargetError::Unknown, "{id:?}");
            assert!(send_keys(id, "x", InputAuthority::Peer).is_err());
            assert!(send_text(
                id,
                "x",
                true,
                InputAuthority::Controller {
                    task_episode: Some(1)
                }
            )
            .is_err());
        }
    }

    fn observe<'a>(activity: &'a str, state: &'a str) -> WaitObservation<'a> {
        WaitObservation {
            activity,
            state,
            task_completed: false,
            completed_at_start: false,
            interrupted: false,
        }
    }

    #[test]
    fn vocabularies_keep_their_authority() {
        // A peer has no task authority: `completed` is not a peer status.
        assert!(parse_wait_target("completed", WaitVocabulary::Peer).is_err());
        assert!(parse_wait_target("unknown", WaitVocabulary::Peer).is_ok());
        assert!(parse_wait_target("unknown", WaitVocabulary::Controller).is_err());
        let completed = parse_wait_target("Completed", WaitVocabulary::Controller).unwrap();
        assert!(completed.wants_completion());
        // Peers keep their exact spelling.
        assert!(parse_wait_target("Idle", WaitVocabulary::Peer).is_err());
    }

    #[test]
    fn completed_is_never_idle_or_exited() {
        let completed = parse_wait_target("completed", WaitVocabulary::Controller).unwrap();
        for (activity, state) in [("idle", "running"), ("done", "running"), ("idle", "exited")] {
            assert_eq!(
                judge_wait(&completed, observe(activity, state)),
                WaitVerdict::Pending,
                "{activity}/{state}"
            );
        }
        let evidence = WaitObservation {
            task_completed: true,
            ..observe("idle", "running")
        };
        assert_eq!(judge_wait(&completed, evidence), WaitVerdict::Matched);
        assert_eq!(
            judge_wait(&completed, observe("blocked", "running")),
            WaitVerdict::NeedsInput
        );
        let interrupted = WaitObservation {
            interrupted: true,
            ..observe("idle", "running")
        };
        assert_eq!(
            judge_wait(&completed, interrupted),
            WaitVerdict::Interrupted
        );
    }

    #[test]
    fn settled_equivalence_is_peer_only() {
        let peer_idle = parse_wait_target("idle", WaitVocabulary::Peer).unwrap();
        assert_eq!(
            judge_wait(&peer_idle, observe("done", "running")),
            WaitVerdict::Matched
        );
        let controller_idle = parse_wait_target("idle", WaitVocabulary::Controller).unwrap();
        assert_eq!(
            judge_wait(&controller_idle, observe("done", "running")),
            WaitVerdict::Pending
        );
        // Controller lifecycle waits see a new completion, never an old one.
        let exited = parse_wait_target("exited", WaitVocabulary::Controller).unwrap();
        let fresh = WaitObservation {
            task_completed: true,
            ..observe("idle", "running")
        };
        assert_eq!(judge_wait(&exited, fresh), WaitVerdict::EpisodeCompleted);
        let stale = WaitObservation {
            completed_at_start: true,
            ..fresh
        };
        assert_eq!(judge_wait(&exited, stale), WaitVerdict::Pending);
        // A peer never reports needs-input: it waits for its exact label.
        let peer_exited = parse_wait_target("exited", WaitVocabulary::Peer).unwrap();
        assert_eq!(
            judge_wait(&peer_exited, observe("blocked", "running")),
            WaitVerdict::Pending
        );
    }

    #[test]
    fn pages_walk_whole_text_on_char_boundaries() {
        let text = "aé".repeat(100);
        let tail = page_text(&text, None, 7);
        assert_eq!(tail.end, text.len());
        assert!(tail.truncated());
        let mut offset = Some(0u64);
        let mut joined = String::new();
        while let Some(next) = offset {
            let page = page_text(&text, Some(next), 7);
            joined.push_str(page.text);
            offset = page.next_offset.map(|value| value as u64);
        }
        assert_eq!(joined, text);
    }

    /// GR7: walking backward with `previous_offset` reaches exactly the
    /// current page's start, so no character between pages is lost, even
    /// when `start - page_bytes` falls inside a multi-byte character.
    #[test]
    fn backward_pages_cover_the_text_without_gaps() {
        let text: String = (0..300)
            .map(|i| match i % 4 {
                0 => 'a',
                1 => 'é',
                2 => '漢',
                _ => '😀',
            })
            .collect();
        for page_bytes in [5usize, 6, 7, 9, 11, 13] {
            let mut page = page_text(&text, None, page_bytes);
            let mut pages = vec![page.text.to_owned()];
            while let Some(previous) = page.previous_offset {
                let next = page_text(&text, Some(previous as u64), page_bytes);
                // No gap: the earlier page starts before the later one and
                // reaches it (it may overlap by less than one character,
                // never stop short of it).
                assert!(
                    next.start < page.start && next.end >= page.start,
                    "page {}..{} leaves a gap before {} ({page_bytes} bytes)",
                    next.start,
                    next.end,
                    page.start
                );
                assert!(next.end - page.start < 4, "overlap stays under one char");
                pages.push(text[next.start..page.start].to_owned());
                page = next;
            }
            pages.reverse();
            assert_eq!(pages.concat(), text, "{page_bytes}-byte pages");
            assert_eq!(page.start, 0);
        }
    }

    /// GR14: one cleaner, two carriage-return profiles. The controller keeps
    /// in-line repaints for its consumers; the peer reads them as lines.
    #[test]
    fn carriage_return_profiles_share_one_cleaner() {
        let raw = "\u{1b}[32mprogress 10%\rprogress 50%\u{1b}[0m\rprogress 99%\r\ndone\u{7}\x08\n\n\n\n\ntail\r";
        assert_eq!(
            clean_terminal_text(raw, CarriageReturns::Keep),
            "progress 10%\rprogress 50%\rprogress 99%\ndone\n\n\ntail\r\n"
        );
        assert_eq!(
            strip_ansi(raw),
            "progress 10%\nprogress 50%\nprogress 99%\ndone\n\n\ntail\n"
        );
        // Escape strings never leak a CR or control through either profile.
        let osc = "a\u{1b}]0;ti\rtle\u{7}b\u{1b}Pdcs\r\u{1b}\\c";
        for profile in [CarriageReturns::Keep, CarriageReturns::AsLineBreaks] {
            assert_eq!(clean_terminal_text(osc, profile), "abc\n");
        }
    }

    #[test]
    fn hostile_ids_are_refused_by_every_read() {
        for id in ["", "../x", "a/b", "a\\b"] {
            assert_eq!(read_target(id).unwrap_err(), TargetError::Unknown);
            assert!(read_output(id, OutputCursor::Tail { max: 16 }).is_err());
            assert!(read_output(
                id,
                OutputCursor::From {
                    offset: Some(0),
                    max: 16
                }
            )
            .is_err());
            assert!(read_transcript(id, TranscriptProfile::Whole { entries: None }).is_err());
        }
    }
}
