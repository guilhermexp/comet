//! Shared path attribution for Projects worked. Presentation and project identity
//! remain with callers; this module only returns checkout-root indices.
use std::path::Path;

use crate::ToolCall;

fn within(path: &str, root: &str) -> bool {
    path.strip_prefix(root)
        .is_some_and(|rest| rest.starts_with('/'))
}

/// Return matching leaf roots in first-contact order, excluding the Chat checkout.
pub fn worked_root_indices<'a>(
    calls: impl IntoIterator<Item = &'a ToolCall>,
    roots: &[&str],
    own_checkout: &Path,
    home_dir: Option<&Path>,
) -> Vec<usize> {
    let roots: Vec<_> = roots
        .iter()
        .map(|root| root.trim().trim_end_matches('/'))
        .collect();
    let own = own_checkout.to_string_lossy();
    let own = own.trim_end_matches('/');
    let leaves: Vec<_> = roots
        .iter()
        .enumerate()
        .filter(|(_, root)| {
            !root.is_empty() && **root != own && !roots.iter().any(|other| within(other, root))
        })
        .map(|(index, root)| (index, *root))
        .collect();
    if leaves.is_empty() {
        return Vec::new();
    }
    let home = home_dir.map(|path| path.to_string_lossy());
    let mut matched = Vec::new();
    let mut seen = vec![false; roots.len()];
    for call in calls {
        let mut consider = |raw: &str, punctuation: bool| {
            let path = raw.trim();
            let path = if punctuation {
                path.trim_end_matches([')', '.', ',', ';', ':'])
            } else {
                path
            };
            let path = path.trim_end_matches('/');
            let expanded;
            let path = if path.starts_with('/') {
                path
            } else if path.starts_with("~/") {
                let Some(home) = home.as_deref() else { return };
                expanded = format!("{}{}", home.trim_end_matches('/'), &path[1..]);
                &expanded
            } else {
                return;
            };
            for &(index, root) in &leaves {
                if !seen[index] && (path == root || within(path, root)) {
                    seen[index] = true;
                    matched.push(index);
                }
            }
        };
        match call {
            ToolCall::ReadFile { path }
            | ToolCall::WriteFile { path, .. }
            | ToolCall::EditFile { path, .. } => consider(path, false),
            ToolCall::ApplyPatch { path: Some(path) } => consider(path, false),
            ToolCall::Search {
                path: Some(path), ..
            }
            | ToolCall::Glob { pattern: path } => {
                for segment in path.split(';') {
                    consider(segment, false);
                }
            }
            ToolCall::Exec { command } => scan_path_tokens(command, |path| consider(path, true)),
            _ => {}
        }
        if matched.len() == leaves.len() {
            break;
        }
    }
    matched
}

fn scan_path_tokens<'a>(text: &'a str, mut on_token: impl FnMut(&'a str)) {
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let prefix = if bytes[i..].starts_with(b"~/") {
            2
        } else if bytes[i] == b'/' {
            1
        } else {
            i += 1;
            continue;
        };
        let start = i;
        let mut end = i + prefix;
        while end < bytes.len() {
            let b = bytes[end];
            if b.is_ascii_whitespace() || b == b'\'' || b == b'"' || b == b'`' || b == b')' {
                break;
            }
            end += 1;
        }
        if let Ok(token) = std::str::from_utf8(&bytes[start..end]) {
            on_token(token);
        }
        i = end;
    }
}
