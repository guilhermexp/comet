//! Canonical textual device references. The durable prompt keeps the link;
//! only the agent-facing copy is expanded into an orchestration hint.
use std::ops::Range;

pub const DEVICE_MENTION_SCHEME: &str = "zeron-device:";

pub struct DeviceMentionLink {
    pub range: Range<usize>,
    pub name: String,
    pub device_id: String,
}

/// Device ids are opaque identities, never paths, queries or markup.
fn valid_device_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

fn device_label(name: &str) -> String {
    let mut escaped = String::with_capacity(name.len());
    for ch in name.chars() {
        match ch {
            '\n' => escaped.push_str("&#10;"),
            '\r' => escaped.push_str("&#13;"),
            '\t' => escaped.push_str("&#9;"),
            '\\' | '[' | ']' | '`' | '*' | '_' | '&' | '<' | '>' | '~' => {
                escaped.push('\\');
                escaped.push(ch);
            }
            _ => escaped.push(ch),
        }
    }
    escaped
}

fn mention_name(name: &str) -> &str {
    if name.is_empty() {
        "Unnamed device"
    } else {
        name
    }
}

/// A canonical device link, or `None` if the opaque id cannot be represented.
pub fn device_mention_link(device_id: &str, name: &str) -> Option<String> {
    valid_device_id(device_id).then(|| {
        format!(
            "[{}]({DEVICE_MENTION_SCHEME}{device_id})",
            device_label(mention_name(name))
        )
    })
}

/// Decode canonical links, but not images, escaped literals or code examples.
pub fn device_mention_links(text: &str) -> Vec<DeviceMentionLink> {
    if !text.contains(DEVICE_MENTION_SCHEME) {
        return Vec::new();
    }
    let mut image_depth = 0;
    let mut open: Option<(Range<usize>, String)> = None;
    let mut name = String::new();
    let mut links = Vec::new();
    for (event, range) in pulldown_cmark::Parser::new(text).into_offset_iter() {
        match &event {
            pulldown_cmark::Event::Start(pulldown_cmark::Tag::Image { .. }) => image_depth += 1,
            pulldown_cmark::Event::End(pulldown_cmark::TagEnd::Image) => image_depth -= 1,
            _ => {}
        }
        if image_depth > 0 {
            continue;
        }
        match event {
            pulldown_cmark::Event::Start(pulldown_cmark::Tag::Link { dest_url, .. }) => {
                if let Some(id) = dest_url.strip_prefix(DEVICE_MENTION_SCHEME)
                    && valid_device_id(id)
                {
                    open = Some((range, id.to_owned()));
                    name.clear();
                }
            }
            pulldown_cmark::Event::Text(text) | pulldown_cmark::Event::Code(text)
                if open.is_some() =>
            {
                name.push_str(&text);
            }
            pulldown_cmark::Event::End(pulldown_cmark::TagEnd::Link) => {
                if let Some((range, device_id)) = open.take()
                    && device_mention_link(&device_id, &name).as_deref()
                        == Some(&text[range.clone()])
                {
                    links.push(DeviceMentionLink {
                        range,
                        name: mention_name(&name).to_owned(),
                        device_id,
                    });
                }
            }
            _ => {}
        }
    }
    links
}

/// Expand for the agent without changing the durable transcript or routing.
pub fn device_mention_prompt(text: &str) -> String {
    let links = device_mention_links(text);
    if links.is_empty() {
        return text.to_owned();
    }
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    for link in links {
        out.push_str(&text[at..link.range.start]);
        // Fence the name as data: downstream Markdown/invocation decoders must
        // never activate a link embedded in a device's user-controlled name.
        let mut run = 0;
        let mut longest = 0;
        for byte in link.name.bytes() {
            if byte == b'`' {
                run += 1;
                longest = longest.max(run);
            } else {
                run = 0;
            }
        }
        let fence = "`".repeat((longest + 1).max(3));
        out.push_str(&format!(
            "the Zeron device named:\n{fence}\n{}\n{fence}\n(device id {}; use this id with existing device tools)",
            link.name, link.device_id
        ));
        at = link.range.end;
    }
    out.push_str(&text[at..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_name_keeps_distinct_ids_and_escaped_label_round_trips() {
        let first = device_mention_link("host-a", "Studio [north]\\desk").unwrap();
        let second = device_mention_link("host-b", "Studio [north]\\desk").unwrap();
        let raw = format!("{first} and {second}");
        let links = device_mention_links(&raw);
        assert_eq!(links.len(), 2);
        assert_eq!(links[0].name, "Studio [north]\\desk");
        assert_eq!(links[0].device_id, "host-a");
        assert_eq!(links[1].device_id, "host-b");
        assert_eq!(&raw[links[0].range.clone()], first);
        let delivered = device_mention_prompt(&raw);
        assert!(delivered.contains("Studio [north]\\desk"));
        assert!(delivered.contains("device id host-a"));
        assert!(delivered.contains("device id host-b"));
    }

    #[test]
    fn markdown_names_and_entities_round_trip_without_changing_identity() {
        for name in [
            "**Desk**",
            "Desk &amp; Lab",
            "[src.rs](zeron-file:src.rs)",
            "Café 🖥 _test_",
            "  two  spaces\tend  ",
            "line\nbreak",
        ] {
            let link = device_mention_link("host-123", name).unwrap();
            let raw = format!("before {link} after");
            let parsed = device_mention_links(&raw);
            assert_eq!(parsed.len(), 1, "{name:?}: {link}");
            assert_eq!(parsed[0].name, name);
            assert_eq!(parsed[0].device_id, "host-123");
            assert_eq!(&raw[parsed[0].range.clone()], link);
        }
    }

    #[test]
    fn names_cannot_activate_other_references_during_harness_delivery() {
        let file = "[src.rs](zeron-file:src.rs)";
        let chat = "[Chat](zeron-chat:chat-1)";
        let command = crate::invocation::Invocation::Command {
            name: "review".into(),
        }
        .link();
        for name in [
            format!("{file} {chat} {command}"),
            format!("```\n\n{file} {command}"),
        ] {
            let link = device_mention_link("host-a", &name).unwrap();
            for harness in [crate::HarnessId::Codex, crate::HarnessId::Omp] {
                let delivered = crate::invocation::harness_prompt(&link, harness);
                assert!(delivered.contains(&name), "{delivered}");
                assert!(delivered.contains("device id host-a"));
                assert!(crate::file_mentions::file_mention_links(&delivered).is_empty());
                assert!(crate::chat_mentions::chat_mention_links(&delivered).is_empty());
                assert!(crate::invocation::invocation_links(&delivered).is_empty());
                assert_eq!(crate::invocation::invocation_prompt(&delivered), delivered);
            }
        }
    }

    #[test]
    fn code_escapes_images_and_invalid_ids_stay_literal() {
        let link = "[Studio](zeron-device:host-a)";
        for raw in [
            format!("`{link}`"),
            format!("```\n{link}\n```"),
            format!("\\{link}"),
            format!("!{link}"),
            "[Studio](zeron-device:host/a)".to_owned(),
            "[Studio](zeron-device:host%2Da)".to_owned(),
            "[](zeron-device:host-a)".to_owned(),
        ] {
            assert!(device_mention_links(&raw).is_empty(), "{raw}");
            assert_eq!(device_mention_prompt(&raw), raw);
        }
        for id in ["", "../other", "host a", "é"] {
            assert!(device_mention_link(id, "Studio").is_none(), "{id}");
        }
        assert!(device_mention_link(&"a".repeat(129), "Studio").is_none());
    }

    #[test]
    fn canonical_text_remains_readable_without_device_expansion() {
        let raw = device_mention_link("host-123", "Laptop").unwrap();
        assert_eq!(raw, "[Laptop](zeron-device:host-123)");
        assert_eq!(crate::file_mentions::file_mention_prompt(&raw), raw);
        assert_eq!(crate::chat_mentions::chat_mention_prompt(&raw), raw);
    }
}
