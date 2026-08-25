use std::ops::Range;

pub const MAX_DECOR_CHARS: usize = 10_000;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DecorStyle {
    pub marker: bool,
    pub heading: Option<u8>,
    pub bold: bool,
    pub italic: bool,
    pub strike: bool,
    pub code: bool,
    pub code_block: bool,
    pub quote: bool,
    pub list_marker: bool,
    pub link: bool,
}

impl DecorStyle {
    fn merge(&mut self, other: Self) {
        self.marker |= other.marker;
        self.heading = self.heading.or(other.heading);
        self.bold |= other.bold;
        self.italic |= other.italic;
        self.strike |= other.strike;
        self.code |= other.code;
        self.code_block |= other.code_block;
        self.quote |= other.quote;
        self.list_marker |= other.list_marker;
        self.link |= other.link;
    }

    fn is_plain(self) -> bool {
        self == Self::default()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecorRange {
    pub range: Range<usize>,
    pub style: DecorStyle,
}

pub fn scan(text: &str) -> Vec<DecorRange> {
    if text.chars().count() > MAX_DECOR_CHARS {
        return Vec::new();
    }

    let mut layers = vec![DecorStyle::default(); text.len()];
    let mut inline_claimed = vec![false; text.len()];
    let mut fence: Option<u8> = None;
    let mut line_start = 0;

    for line_with_newline in text.split_inclusive('\n') {
        let line = line_with_newline
            .strip_suffix('\n')
            .unwrap_or(line_with_newline);
        let line_end = line_start + line.len();
        let indent = line.bytes().take_while(|byte| *byte == b' ').count();
        let body = &line[indent.min(line.len())..];
        let fence_marker = (indent <= 3)
            .then(|| body.as_bytes().first().copied())
            .flatten()
            .filter(|marker| {
                (*marker == b'`' || *marker == b'~') && body.as_bytes().starts_with(&[*marker; 3])
            });

        if let Some(open_marker) = fence {
            apply_style(
                &mut layers,
                line_start..line_end,
                DecorStyle {
                    code_block: true,
                    ..DecorStyle::default()
                },
            );
            inline_claimed[line_start..line_end].fill(true);
            if fence_marker == Some(open_marker) {
                apply_style(
                    &mut layers,
                    line_start + indent..line_start + indent + 3,
                    DecorStyle {
                        marker: true,
                        ..DecorStyle::default()
                    },
                );
                fence = None;
            }
        } else if let Some(marker) = fence_marker {
            apply_style(
                &mut layers,
                line_start..line_end,
                DecorStyle {
                    code_block: true,
                    ..DecorStyle::default()
                },
            );
            apply_style(
                &mut layers,
                line_start + indent..line_start + indent + 3,
                DecorStyle {
                    marker: true,
                    ..DecorStyle::default()
                },
            );
            inline_claimed[line_start..line_end].fill(true);
            fence = Some(marker);
        } else if indent <= 3 {
            let hashes = body.bytes().take_while(|byte| *byte == b'#').count();
            if (1..=6).contains(&hashes) && body.as_bytes().get(hashes) == Some(&b' ') {
                apply_style(
                    &mut layers,
                    line_start + indent..line_end,
                    DecorStyle {
                        heading: Some(hashes as u8),
                        ..DecorStyle::default()
                    },
                );
                apply_style(
                    &mut layers,
                    line_start + indent..line_start + indent + hashes + 1,
                    DecorStyle {
                        marker: true,
                        ..DecorStyle::default()
                    },
                );
            } else if is_horizontal_rule(body) {
                apply_style(
                    &mut layers,
                    line_start + indent..line_end,
                    DecorStyle {
                        marker: true,
                        ..DecorStyle::default()
                    },
                );
            } else if body.starts_with('>')
                && body
                    .as_bytes()
                    .get(1)
                    .is_none_or(|byte| byte.is_ascii_whitespace())
            {
                apply_style(
                    &mut layers,
                    line_start + indent..line_end,
                    DecorStyle {
                        quote: true,
                        ..DecorStyle::default()
                    },
                );
                let marker_end = line_start + indent + 1 + usize::from(body.starts_with("> "));
                apply_style(
                    &mut layers,
                    line_start + indent..marker_end,
                    DecorStyle {
                        marker: true,
                        ..DecorStyle::default()
                    },
                );
            } else if let Some(marker_len) = list_marker_len(body) {
                apply_style(
                    &mut layers,
                    line_start + indent..line_start + indent + marker_len,
                    DecorStyle {
                        marker: true,
                        list_marker: true,
                        ..DecorStyle::default()
                    },
                );
            }
        }

        line_start += line_with_newline.len();
    }

    apply_delimited(
        text,
        b"`",
        DecorStyle {
            code: true,
            ..DecorStyle::default()
        },
        &mut inline_claimed,
        &mut layers,
    );
    apply_links(text, &mut inline_claimed, &mut layers);
    apply_delimited(
        text,
        b"**",
        DecorStyle {
            bold: true,
            ..DecorStyle::default()
        },
        &mut inline_claimed,
        &mut layers,
    );
    apply_delimited(
        text,
        b"__",
        DecorStyle {
            bold: true,
            ..DecorStyle::default()
        },
        &mut inline_claimed,
        &mut layers,
    );
    apply_delimited(
        text,
        b"~~",
        DecorStyle {
            strike: true,
            ..DecorStyle::default()
        },
        &mut inline_claimed,
        &mut layers,
    );
    apply_italic(text, b'*', &mut inline_claimed, &mut layers);
    apply_italic(text, b'_', &mut inline_claimed, &mut layers);

    flatten(layers)
}

fn is_horizontal_rule(line: &str) -> bool {
    let mut marker = None;
    let mut count = 0;
    for byte in line.trim_end().bytes() {
        if byte.is_ascii_whitespace() {
            continue;
        }
        if !matches!(byte, b'-' | b'_' | b'*') || marker.is_some_and(|seen| seen != byte) {
            return false;
        }
        marker = Some(byte);
        count += 1;
    }
    count >= 3
}

fn list_marker_len(line: &str) -> Option<usize> {
    let bytes = line.as_bytes();
    if matches!(bytes.first(), Some(b'-' | b'*' | b'+'))
        && bytes.get(1).is_some_and(u8::is_ascii_whitespace)
    {
        return Some(2);
    }

    let digits = bytes
        .iter()
        .take_while(|byte| byte.is_ascii_digit())
        .count();
    if digits > 0
        && matches!(bytes.get(digits), Some(b'.' | b')'))
        && bytes.get(digits + 1).is_some_and(u8::is_ascii_whitespace)
    {
        Some(digits + 2)
    } else {
        None
    }
}

fn apply_links(text: &str, claimed: &mut [bool], layers: &mut [DecorStyle]) {
    let bytes = text.as_bytes();
    let mut cursor = 0;
    while cursor < bytes.len() {
        let Some(open_rel) = bytes[cursor..].iter().position(|byte| *byte == b'[') else {
            break;
        };
        let open = cursor + open_rel;
        let Some(middle_rel) = find_bytes(&bytes[open + 1..], b"](") else {
            break;
        };
        let middle = open + 1 + middle_rel;
        let url_start = middle + 2;
        let Some(close_rel) = bytes[url_start..]
            .iter()
            .position(|byte| *byte == b')' || *byte == b'\n')
        else {
            break;
        };
        let close = url_start + close_rel;
        if bytes[close] == b'\n' {
            cursor = close + 1;
            continue;
        }
        let end = close + 1;
        if middle > open + 1
            && close > url_start
            && !claimed[open..end].iter().any(|claimed| *claimed)
        {
            claimed[open..end].fill(true);
            apply_style(
                layers,
                open + 1..middle,
                DecorStyle {
                    link: true,
                    ..DecorStyle::default()
                },
            );
            let marker_style = DecorStyle {
                marker: true,
                ..DecorStyle::default()
            };
            apply_style(layers, open..open + 1, marker_style);
            apply_style(layers, middle..end, marker_style);
        }
        cursor = end;
    }
}

fn apply_italic(text: &str, delimiter: u8, claimed: &mut [bool], layers: &mut [DecorStyle]) {
    let bytes = text.as_bytes();
    let mut cursor = 0;
    while cursor < bytes.len() {
        let Some(open_rel) = bytes[cursor..].iter().position(|byte| *byte == delimiter) else {
            break;
        };
        let open = cursor + open_rel;
        let open_allowed = !is_word_char(char_before(text, open))
            && char_after(text, open + 1).is_some_and(|ch| !ch.is_whitespace())
            && bytes.get(open + 1) != Some(&delimiter);
        if !open_allowed {
            cursor = open + 1;
            continue;
        }

        let mut search = open + 1;
        let mut matched = None;
        while let Some(close_rel) = bytes[search..].iter().position(|byte| *byte == delimiter) {
            let close = search + close_rel;
            if bytes[open + 1..close].contains(&b'\n') {
                break;
            }
            let close_allowed = char_before(text, close).is_some_and(|ch| !ch.is_whitespace())
                && !is_word_char(char_after(text, close + 1))
                && bytes.get(close + 1) != Some(&delimiter);
            if close_allowed {
                matched = Some(close);
                break;
            }
            search = close + 1;
        }

        let Some(close) = matched else {
            cursor = open + 1;
            continue;
        };
        let end = close + 1;
        if !claimed[open..end].iter().any(|claimed| *claimed) {
            claimed[open..end].fill(true);
            apply_style(
                layers,
                open + 1..close,
                DecorStyle {
                    italic: true,
                    ..DecorStyle::default()
                },
            );
            let marker_style = DecorStyle {
                marker: true,
                ..DecorStyle::default()
            };
            apply_style(layers, open..open + 1, marker_style);
            apply_style(layers, close..end, marker_style);
        }
        cursor = end;
    }
}

fn char_before(text: &str, byte_index: usize) -> Option<char> {
    text.get(..byte_index)?.chars().next_back()
}

fn char_after(text: &str, byte_index: usize) -> Option<char> {
    text.get(byte_index..)?.chars().next()
}

fn is_word_char(ch: Option<char>) -> bool {
    ch.is_some_and(|ch| ch.is_alphanumeric() || ch == '_')
}

fn apply_delimited(
    text: &str,
    delimiter: &[u8],
    content_style: DecorStyle,
    claimed: &mut [bool],
    layers: &mut [DecorStyle],
) {
    let bytes = text.as_bytes();
    let mut cursor = 0;
    while cursor + delimiter.len() <= bytes.len() {
        let Some(open_rel) = find_bytes(&bytes[cursor..], delimiter) else {
            break;
        };
        let open = cursor + open_rel;
        let content_start = open + delimiter.len();
        let Some(close_rel) = find_bytes(&bytes[content_start..], delimiter) else {
            break;
        };
        let close = content_start + close_rel;
        let end = close + delimiter.len();
        let content = &text[content_start..close];
        let valid_content = !content.contains(['\n', '\r'])
            && content.chars().next().is_some_and(|ch| !ch.is_whitespace())
            && content
                .chars()
                .next_back()
                .is_some_and(|ch| !ch.is_whitespace());
        if !valid_content {
            cursor = content_start;
            continue;
        }
        if !claimed[open..end].iter().any(|claimed| *claimed) {
            claimed[open..end].fill(true);
            apply_style(layers, content_start..close, content_style);
            let marker_style = DecorStyle {
                marker: true,
                ..DecorStyle::default()
            };
            apply_style(layers, open..content_start, marker_style);
            apply_style(layers, close..end, marker_style);
        }
        cursor = end;
    }
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn apply_style(layers: &mut [DecorStyle], range: Range<usize>, style: DecorStyle) {
    for layer in &mut layers[range] {
        layer.merge(style);
    }
}

fn flatten(layers: Vec<DecorStyle>) -> Vec<DecorRange> {
    let mut ranges = Vec::new();
    let mut start = 0;
    while start < layers.len() {
        let style = layers[start];
        let mut end = start + 1;
        while end < layers.len() && layers[end] == style {
            end += 1;
        }
        if !style.is_plain() {
            ranges.push(DecorRange {
                range: start..end,
                style,
            });
        }
        start = end;
    }
    ranges
}

#[cfg(test)]
mod tests {
    use super::{DecorStyle, MAX_DECOR_CHARS, scan};

    fn style_for<'a>(text: &str, ranges: &'a [super::DecorRange], needle: &str) -> &'a DecorStyle {
        let start = text.find(needle).expect("fixture contains needle");
        ranges
            .iter()
            .find(|range| range.range.start <= start && range.range.end >= start + needle.len())
            .map(|range| &range.style)
            .expect("needle is covered by one coalesced decoration range")
    }

    #[test]
    fn fenced_code_state_carries_between_lines() {
        let text = "```rust\nlet answer = 42;\n```\nafter";
        let ranges = scan(text);

        assert!(style_for(text, &ranges, "let answer = 42;").code_block);
        assert!(!ranges.iter().any(|range| {
            range.range.start <= text.find("after").unwrap()
                && range.range.end > text.find("after").unwrap()
        }));
    }

    #[test]
    fn nested_heading_and_bold_flatten_to_one_merged_range() {
        let text = "## **Title**";
        let ranges = scan(text);
        let style = style_for(text, &ranges, "Title");

        assert_eq!(style.heading, Some(2));
        assert!(style.bold);
        assert_eq!(
            ranges
                .iter()
                .filter(|range| {
                    let title = text.find("Title").unwrap();
                    range.range.start <= title && range.range.end >= title + "Title".len()
                })
                .count(),
            1
        );
    }

    #[test]
    fn snake_case_is_not_italic() {
        let text = "snake_case_name";
        assert!(!scan(text).iter().any(|range| range.style.italic));
    }

    #[test]
    fn unclosed_markers_do_not_style_content() {
        let text = "before **marker and `code";
        let ranges = scan(text);

        assert!(
            !ranges
                .iter()
                .any(|range| range.style.bold || range.style.code)
        );
    }

    #[test]
    fn inputs_above_the_cap_skip_decoration() {
        let text = format!("**{}**", "x".repeat(MAX_DECOR_CHARS));
        assert!(text.chars().count() > MAX_DECOR_CHARS);
        assert!(scan(&text).is_empty());
    }

    #[test]
    fn cap_counts_characters_not_utf8_bytes() {
        let text = format!("**{}**", "é".repeat(MAX_DECOR_CHARS - 4));
        assert_eq!(text.chars().count(), MAX_DECOR_CHARS);

        assert!(style_for(&text, &scan(&text), "é").bold);
    }

    #[test]
    fn scan_is_pure_and_idempotent_by_construction() {
        let text = "# [**Title**](https://example.com)\n> *quote*\n- item";
        let original = text.to_owned();
        let first = scan(text);
        let second = scan(text);

        assert_eq!(text, original);
        assert_eq!(first, second);
    }

    #[test]
    fn horizontal_rule_quote_and_list_markers_are_classified() {
        let text = "---\n> quote\n- bullet\n12) numbered";
        let ranges = scan(text);

        assert!(style_for(text, &ranges, "---").marker);
        assert!(style_for(text, &ranges, "quote").quote);
        assert!(style_for(text, &ranges, ">").marker);
        let bullet = text.find("- bullet").unwrap();
        assert!(ranges.iter().any(|range| {
            range.range.start <= bullet && range.range.end > bullet && range.style.list_marker
        }));
        assert!(style_for(text, &ranges, "12)").list_marker);
    }

    #[test]
    fn inline_code_link_strike_and_italic_are_classified() {
        let text = "`code` [label](https://example.com) ~~gone~~ *em*";
        let ranges = scan(text);

        assert!(style_for(text, &ranges, "code").code);
        assert!(style_for(text, &ranges, "label").link);
        let target = style_for(text, &ranges, "https://example.com");
        assert!(target.marker);
        assert!(!target.link);
        assert!(style_for(text, &ranges, "gone").strike);
        assert!(style_for(text, &ranges, "em").italic);
    }

    #[test]
    fn inline_code_claims_characters_before_bold() {
        let text = "`**literal**`";
        let style = style_for(text, &scan(text), "literal").to_owned();

        assert!(style.code);
        assert!(!style.bold);
    }

    #[test]
    fn inline_delimiters_do_not_cross_lines_or_pad_content() {
        for text in ["**a\nb**", "`a\nb`", "** a **", "~~ gone ~~", "* em *"] {
            let ranges = scan(text);
            assert!(
                !ranges.iter().any(|range| {
                    range.style.bold || range.style.code || range.style.strike || range.style.italic
                }),
                "invalid inline fixture was styled: {text:?}"
            );
        }
    }
}
