use std::ops::Range;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UrlChipKind {
    GitHub,
    YouTube,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum UrlSegment<'a> {
    Text(&'a str),
    Chip {
        url: &'a str,
        label: String,
        kind: UrlChipKind,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UrlChipSpan {
    pub range: Range<usize>,
    pub url: String,
    pub kind: UrlChipKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct UrlProjection {
    pub text: String,
    pub chips: Vec<UrlChipSpan>,
    replacements: Vec<(Range<usize>, Range<usize>)>,
}

impl UrlProjection {
    pub(crate) fn project_range(&self, range: Range<usize>) -> Range<usize> {
        self.project_offset(range.start)..self.project_offset(range.end)
    }

    fn project_offset(&self, offset: usize) -> usize {
        let mut delta = 0isize;
        for (source, display) in &self.replacements {
            if offset < source.start {
                break;
            }
            if offset <= source.end {
                if offset == source.start {
                    return display.start;
                }
                return display.end;
            }
            delta += display.len() as isize - source.len() as isize;
        }
        offset.saturating_add_signed(delta)
    }
}

pub(crate) fn project_urls(text: &str) -> Option<UrlProjection> {
    let segments = segment_urls(text);
    if !segments
        .iter()
        .any(|segment| matches!(segment, UrlSegment::Chip { .. }))
    {
        return None;
    }

    let mut projected = String::with_capacity(text.len());
    let mut chips = Vec::new();
    let mut replacements = Vec::new();
    let mut source_at = 0;
    for segment in segments {
        match segment {
            UrlSegment::Text(plain) => {
                projected.push_str(plain);
                source_at += plain.len();
            }
            UrlSegment::Chip { url, label, kind } => {
                let source = source_at..source_at + url.len();
                let display_start = projected.len();
                if kind == UrlChipKind::GitHub {
                    // Reserve one icon-width before the label. The transcript
                    // paints its existing git-branch asset into this space.
                    projected.push_str("  ");
                }
                projected.push_str(&label);
                let display = display_start..projected.len();
                chips.push(UrlChipSpan {
                    range: display.clone(),
                    url: url.to_owned(),
                    kind,
                });
                replacements.push((source, display));
                source_at += url.len();
            }
        }
    }

    Some(UrlProjection {
        text: projected,
        chips,
        replacements,
    })
}

pub(crate) fn segment_urls(text: &str) -> Vec<UrlSegment<'_>> {
    let mut segments = Vec::new();
    let mut plain_start = 0;
    let mut search_from = 0;

    while let Some(url_start) = next_url_start(text, search_from) {
        let whitespace_end = text[url_start..]
            .find(char::is_whitespace)
            .map_or(text.len(), |offset| url_start + offset);
        let adjacent_url_start = next_url_start(text, url_start + 1).filter(|next_start| {
            *next_start < whitespace_end
                && text[..*next_start]
                    .chars()
                    .next_back()
                    .is_some_and(is_trailing_punctuation)
        });
        let token_end = adjacent_url_start.unwrap_or(whitespace_end);
        let url_end = trim_trailing_punctuation(&text[url_start..token_end]) + url_start;

        if let Some((kind, label)) = classify_url(&text[url_start..url_end]) {
            if plain_start < url_start {
                segments.push(UrlSegment::Text(&text[plain_start..url_start]));
            }
            segments.push(UrlSegment::Chip {
                url: &text[url_start..url_end],
                label,
                kind,
            });
            plain_start = url_end;
        }

        search_from = adjacent_url_start.unwrap_or(whitespace_end);
    }

    if plain_start < text.len() || segments.is_empty() {
        segments.push(UrlSegment::Text(&text[plain_start..]));
    }
    segments
}

fn next_url_start(text: &str, from: usize) -> Option<usize> {
    text[from..].char_indices().find_map(|(offset, _)| {
        let start = from + offset;
        let is_boundary = start == 0
            || text[..start]
                .chars()
                .next_back()
                .is_some_and(|ch| !ch.is_ascii_alphanumeric() && ch != '_');
        let remainder = &text[start..];
        let has_scheme = remainder
            .get(.."https://".len())
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("https://"))
            || remainder
                .get(.."http://".len())
                .is_some_and(|prefix| prefix.eq_ignore_ascii_case("http://"));
        (is_boundary && has_scheme).then_some(start)
    })
}

fn trim_trailing_punctuation(url: &str) -> usize {
    url.trim_end_matches(is_trailing_punctuation).len()
}

fn is_trailing_punctuation(ch: char) -> bool {
    matches!(ch, '.' | ',' | ';' | ':' | '!' | '?' | ')' | ']')
}

fn classify_url(url: &str) -> Option<(UrlChipKind, String)> {
    let after_scheme = if url
        .get(.."https://".len())
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("https://"))
    {
        &url["https://".len()..]
    } else if url
        .get(.."http://".len())
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("http://"))
    {
        &url["http://".len()..]
    } else {
        return None;
    };
    let host_end = after_scheme
        .find(['/', '?', '#'])
        .unwrap_or(after_scheme.len());
    let raw_host = &after_scheme[..host_end];
    let host = if raw_host
        .get(.."www.".len())
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("www."))
    {
        &raw_host["www.".len()..]
    } else {
        raw_host
    };

    if host.eq_ignore_ascii_case("github.com") {
        let path = after_scheme[host_end..]
            .strip_prefix('/')
            .unwrap_or_default()
            .split(['?', '#'])
            .next()
            .unwrap_or_default();
        let mut parts = path.split('/').filter(|part| !part.is_empty());
        let label = match (parts.next(), parts.next()) {
            (Some(owner), Some(repo)) => format!("{owner}/{repo}"),
            _ => "GitHub".to_owned(),
        };
        Some((UrlChipKind::GitHub, label))
    } else if host.eq_ignore_ascii_case("youtube.com") || host.eq_ignore_ascii_case("youtu.be") {
        Some((UrlChipKind::YouTube, "YouTube".to_owned()))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::{UrlChipKind, UrlSegment, segment_urls};

    #[test]
    fn leaves_trailing_punctuation_outside_github_chip() {
        assert_eq!(
            segment_urls("see https://github.com/rust-lang/rust.,;:!?)] next"),
            vec![
                UrlSegment::Text("see "),
                UrlSegment::Chip {
                    url: "https://github.com/rust-lang/rust",
                    label: "rust-lang/rust".into(),
                    kind: UrlChipKind::GitHub,
                },
                UrlSegment::Text(".,;:!?)] next"),
            ]
        );
    }

    #[test]
    fn uses_first_two_github_path_segments_for_label() {
        assert_eq!(
            segment_urls("https://github.com/owner/repo/issues/42"),
            vec![UrlSegment::Chip {
                url: "https://github.com/owner/repo/issues/42",
                label: "owner/repo".into(),
                kind: UrlChipKind::GitHub,
            }]
        );
    }

    #[test]
    fn falls_back_for_bare_github_host() {
        assert_eq!(
            segment_urls("https://github.com"),
            vec![UrlSegment::Chip {
                url: "https://github.com",
                label: "GitHub".into(),
                kind: UrlChipKind::GitHub,
            }]
        );
    }

    #[test]
    fn labels_youtube_hosts_consistently() {
        assert_eq!(
            segment_urls("https://youtube.com/watch?v=abc https://youtu.be/abc"),
            vec![
                UrlSegment::Chip {
                    url: "https://youtube.com/watch?v=abc",
                    label: "YouTube".into(),
                    kind: UrlChipKind::YouTube,
                },
                UrlSegment::Text(" "),
                UrlSegment::Chip {
                    url: "https://youtu.be/abc",
                    label: "YouTube".into(),
                    kind: UrlChipKind::YouTube,
                },
            ]
        );
    }

    #[test]
    fn keeps_other_urls_as_plain_text() {
        let text = "docs https://example.com/owner/repo and github.com/no-scheme/repo";
        assert_eq!(segment_urls(text), vec![UrlSegment::Text(text)]);
    }

    #[test]
    fn finds_supported_urls_adjacent_to_other_urls_by_punctuation() {
        assert_eq!(
            segment_urls(
                "https://example.com,https://github.com/rust-lang/rust;https://youtu.be/abc"
            ),
            vec![
                UrlSegment::Text("https://example.com,"),
                UrlSegment::Chip {
                    url: "https://github.com/rust-lang/rust",
                    label: "rust-lang/rust".into(),
                    kind: UrlChipKind::GitHub,
                },
                UrlSegment::Text(";"),
                UrlSegment::Chip {
                    url: "https://youtu.be/abc",
                    label: "YouTube".into(),
                    kind: UrlChipKind::YouTube,
                },
            ]
        );
    }

    #[test]
    fn matches_scheme_and_www_prefix_case_insensitively() {
        assert_eq!(
            segment_urls("HTTPS://github.com/owner/repo https://WWW.YouTube.com/watch?v=x"),
            vec![
                UrlSegment::Chip {
                    url: "HTTPS://github.com/owner/repo",
                    label: "owner/repo".into(),
                    kind: UrlChipKind::GitHub,
                },
                UrlSegment::Text(" "),
                UrlSegment::Chip {
                    url: "https://WWW.YouTube.com/watch?v=x",
                    label: "YouTube".into(),
                    kind: UrlChipKind::YouTube,
                },
            ]
        );
    }

    #[test]
    fn preserves_nested_urls_inside_a_query_value() {
        assert_eq!(
            segment_urls("https://github.com/owner/repo?next=https://example.com/docs"),
            vec![UrlSegment::Chip {
                url: "https://github.com/owner/repo?next=https://example.com/docs",
                label: "owner/repo".into(),
                kind: UrlChipKind::GitHub,
            }]
        );
    }

    #[test]
    fn projects_chip_labels_and_remaps_following_ranges() {
        let text = "see https://github.com/rust-lang/rust. then src/lib.rs";
        let source_range = text.find("src/lib.rs").unwrap()..text.len();
        let projection = super::project_urls(text).expect("supported URL projects");

        assert_eq!(projection.text, "see   rust-lang/rust. then src/lib.rs");
        assert_eq!(projection.chips.len(), 1);
        let chip = &projection.chips[0];
        assert_eq!(&projection.text[chip.range.clone()], "  rust-lang/rust");
        assert_eq!(chip.url, "https://github.com/rust-lang/rust");
        assert_eq!(chip.kind, UrlChipKind::GitHub);
        assert_eq!(
            &projection.text[projection.project_range(source_range)],
            "src/lib.rs"
        );
    }
}
