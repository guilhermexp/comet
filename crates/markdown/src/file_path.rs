//! Pure path heuristics shared by inline autolinking and the desktop file
//! preview: what a token looks like, not whether it exists.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewKind {
    Markdown,
    Code,
    Html,
    Image,
    Pdf,
    Video,
    Data,
    Unsupported,
}

pub fn strip_line_col(path: &str) -> &str {
    let trimmed = path.trim();
    if trimmed.contains("://") {
        return trimmed;
    }
    if let Some((base, suffix)) = trimmed.rsplit_once(':') {
        let numeric = |value: &str| !value.is_empty() && value.bytes().all(|c| c.is_ascii_digit());
        let range = suffix.split_once('-').or_else(|| suffix.split_once('+'));
        if numeric(suffix) || range.is_some_and(|(a, b)| numeric(a) && numeric(b)) {
            if let Some((inner_base, inner_suffix)) = base.rsplit_once(':') {
                if numeric(inner_suffix) || inner_suffix == "raw" {
                    return inner_base;
                }
            }
            return base;
        }
    }
    trimmed
}

pub fn is_file_path_candidate(token: &str) -> Option<&str> {
    let trimmed = token.trim();
    if trimmed.is_empty()
        || trimmed.contains(char::is_whitespace)
        || trimmed.contains("://")
        || trimmed.starts_with('#')
        || trimmed.starts_with("mailto:")
        || trimmed.starts_with("tel:")
    {
        return None;
    }
    // Check invalid characters for a path
    if trimmed
        .chars()
        .any(|c| matches!(c, '<' | '>' | '"' | '|' | '?' | '*' | '{' | '}' | '(' | ')'))
    {
        return None;
    }
    let clean = strip_line_col(trimmed);
    if clean.starts_with("~/") || clean == "~" {
        return Some(trimmed);
    }
    if clean.starts_with('/') && clean.len() > 1 && !clean.starts_with("//") {
        return Some(trimmed);
    }
    if clean.starts_with("./") || clean.starts_with("../") {
        return Some(trimmed);
    }
    if clean.contains('/') {
        let filename = clean.rsplit_once('/').map(|(_, f)| f).unwrap_or(clean);
        if classify_preview_kind(filename) != PreviewKind::Unsupported {
            return Some(trimmed);
        }
        if let Some((_, ext)) = filename.rsplit_once('.') {
            if !ext.is_empty() && ext.len() <= 8 && ext.chars().all(|c| c.is_ascii_alphanumeric()) {
                return Some(trimmed);
            }
        }
    } else if classify_preview_kind(clean) != PreviewKind::Unsupported {
        return Some(trimmed);
    }
    None
}

pub fn classify_preview_kind(path: &str) -> PreviewKind {
    let clean = strip_line_col(path);
    let filename = clean.rsplit_once('/').map(|(_, f)| f).unwrap_or(clean);
    if matches!(
        filename,
        "Makefile"
            | "Dockerfile"
            | "Containerfile"
            | "LICENSE"
            | "LICENCE"
            | "COPYING"
            | "Gemfile"
            | "Rakefile"
            | "Procfile"
            | "Vagrantfile"
            | "Justfile"
            | "Brewfile"
            | "Fastfile"
            | "CMakeLists.txt"
            | ".gitignore"
            | ".gitattributes"
            | ".gitmodules"
            | ".env"
            | ".editorconfig"
            | ".prettierrc"
            | ".eslintrc"
            | ".npmrc"
    ) {
        return PreviewKind::Code;
    }
    let extension = clean
        .rsplit_once('.')
        .map(|(_, extension)| extension.to_ascii_lowercase())
        .unwrap_or_default();
    match extension.as_str() {
        "md" | "mdx" | "markdown" => PreviewKind::Markdown,
        "html" | "htm" => PreviewKind::Html,
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "svg" => PreviewKind::Image,
        "pdf" => PreviewKind::Pdf,
        // WebKit renders these as a media document with its own player; see
        // the Video arm in `load_preview` for why they skip the byte cap.
        "mp4" | "mov" | "m4v" | "webm" => PreviewKind::Video,
        "csv" | "tsv" | "xls" | "xlsx" => PreviewKind::Data,
        "rs" | "js" | "jsx" | "ts" | "tsx" | "py" | "go" | "json" | "jsonc" | "sh" | "bash"
        | "zsh" | "toml" | "css" | "scss" | "yaml" | "yml" | "c" | "h" | "cpp" | "cc" | "cxx"
        | "hpp" | "cs" | "java" | "kt" | "swift" | "rb" | "php" | "sql" | "lua" | "nix"
        | "make" | "txt" | "log" | "xml" | "ini" | "conf" | "env" | "lock" | "patch" | "diff"
        | "proto" | "graphql" | "gql" | "vue" | "svelte" | "zig" | "dart" | "scala" | "r"
        | "plist" | "properties" | "gradle" | "cmake" | "mk" | "json5" | "jsonl" | "ndjson" => {
            PreviewKind::Code
        }
        _ => PreviewKind::Unsupported,
    }
}
