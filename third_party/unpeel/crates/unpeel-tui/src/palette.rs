//! Command palette (the desktop's ⌘K), same model as
//! `CommandPaletteView.swift`: sessions first, then projects, preset
//! launches, and app commands — fuzzy-scored, with archived sessions
//! appended only when nothing else matches (the "search archive" case).

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Session,
    Project,
    Launch,
    Command,
}

impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Kind::Session => "Session",
            Kind::Project => "Project",
            Kind::Launch => "Launch",
            Kind::Command => "Command",
        }
    }
}

#[derive(Clone, Debug)]
pub enum Action {
    SelectSession(String),
    SelectProject(String),
    /// Launch a command in the selected session's project.
    Launch(String),
    OpenSettings,
    ToggleFold,
    NewTerminal(String),
}

#[derive(Clone, Debug)]
pub struct Item {
    pub kind: Kind,
    pub title: String,
    pub subtitle: String,
    /// Extra text the matcher may hit (a session's command, a project path).
    pub keywords: String,
    /// Command whose CLI tint colors the row (launch/session rows).
    pub icon_command: String,
    pub action: Action,
}

/// Subsequence fuzzy score: consecutive and word-start hits rank higher,
/// mirroring `PaletteFuzzy`. None when the query isn't a subsequence.
pub fn score(query: &str, haystack: &str) -> Option<i32> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return Some(0);
    }
    let hay: Vec<char> = haystack.to_lowercase().chars().collect();
    let mut total = 0i32;
    let mut index = 0usize;
    let mut last_hit: Option<usize> = None;
    for needle in query.chars() {
        let found = hay[index..].iter().position(|c| *c == needle)? + index;
        let mut points = 1;
        if last_hit == Some(found.wrapping_sub(1)) {
            points += 4; // consecutive run
        }
        if found == 0
            || matches!(
                hay.get(found - 1),
                Some(' ') | Some('-') | Some('/') | Some('_')
            )
        {
            points += 3; // word start
        }
        total += points;
        last_hit = Some(found);
        index = found + 1;
    }
    // Prefer tighter matches in shorter strings.
    Some(total * 100 / (haystack.chars().count().max(1) as i32 + 10))
}

pub fn best_score(query: &str, item: &Item) -> Option<i32> {
    let title = score(query, &item.title).map(|s| s * 2);
    let subtitle = score(query, &item.subtitle);
    let keywords = score(query, &item.keywords);
    [title, subtitle, keywords].into_iter().flatten().max()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subsequence_matching() {
        assert!(score("cld", "claude session").is_some());
        assert!(score("zzz", "claude session").is_none());
        assert!(score("", "anything").is_some());
    }

    #[test]
    fn consecutive_and_word_starts_rank_higher() {
        let exact = score("claude", "claude").unwrap();
        let scattered = score("claude", "c l a u d e").unwrap();
        assert!(exact > scattered, "{exact} !> {scattered}");
        let word_start = score("ns", "new session").unwrap();
        let mid_word = score("ns", "lens shift").unwrap();
        assert!(word_start > mid_word, "{word_start} !> {mid_word}");
    }

    #[test]
    fn title_outweighs_keywords() {
        let item = Item {
            kind: Kind::Session,
            title: "release workflow".into(),
            subtitle: "unpeel".into(),
            keywords: "claude --dangerously-skip-permissions".into(),
            icon_command: String::new(),
            action: Action::SelectSession("s1".into()),
        };
        let title_hit = best_score("release", &item).unwrap();
        let keyword_hit = best_score("dangerously", &item).unwrap();
        assert!(title_hit > keyword_hit, "{title_hit} !> {keyword_hit}");
        assert!(best_score("zzzz", &item).is_none());
    }
}
