//! Subsequence fuzzy scoring, the same model as the TUI palette and the
//! desktop's `PaletteFuzzy`: consecutive and word-start hits rank higher.

/// Subsequence fuzzy score. `None` when the query isn't a subsequence of
/// the haystack; higher is a better match. An empty query matches
/// everything with score 0.
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
}
