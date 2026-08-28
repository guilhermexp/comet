use std::collections::HashMap;

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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PreviewDisplayMode {
    #[default]
    SidePeek,
    FullPage,
}

impl PreviewDisplayMode {
    pub fn toggled(self) -> Self {
        match self {
            Self::SidePeek => Self::FullPage,
            Self::FullPage => Self::SidePeek,
        }
    }
}

pub fn classify_preview_kind(path: &str) -> PreviewKind {
    let extension = path
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
        | "make" | "txt" | "log" | "xml" | "ini" | "conf" | "env" => PreviewKind::Code,
        _ => PreviewKind::Unsupported,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewTab {
    pub relative_path: String,
}

#[derive(Debug, Default, Clone)]
struct ContextTabs {
    paths: Vec<String>,
    active: Option<String>,
}

#[derive(Debug, Default, Clone)]
pub struct PreviewTabs {
    contexts: HashMap<String, ContextTabs>,
}

impl PreviewTabs {
    pub fn open(&mut self, context: &str, relative_path: &str) {
        let tabs = self.contexts.entry(context.to_string()).or_default();
        if !tabs.paths.iter().any(|path| path == relative_path) {
            tabs.paths.push(relative_path.to_string());
        }
        tabs.active = Some(relative_path.to_string());
    }

    pub fn select(&mut self, context: &str, relative_path: &str) {
        if let Some(tabs) = self.contexts.get_mut(context)
            && tabs.paths.iter().any(|path| path == relative_path)
        {
            tabs.active = Some(relative_path.to_string());
        }
    }

    pub fn close(&mut self, context: &str, relative_path: &str) {
        let Some(tabs) = self.contexts.get_mut(context) else {
            return;
        };
        let Some(index) = tabs.paths.iter().position(|path| path == relative_path) else {
            return;
        };
        tabs.paths.remove(index);
        if tabs.active.as_deref() == Some(relative_path) {
            tabs.active = tabs
                .paths
                .get(index.saturating_sub(1))
                .or_else(|| tabs.paths.first())
                .cloned();
        }
    }

    pub fn paths(&self, context: &str) -> &[String] {
        self.contexts
            .get(context)
            .map(|tabs| tabs.paths.as_slice())
            .unwrap_or_default()
    }

    pub fn active_path(&self, context: &str) -> Option<&str> {
        self.contexts
            .get(context)
            .and_then(|tabs| tabs.active.as_deref())
    }
}

#[cfg(test)]
mod tests {
    use super::{PreviewDisplayMode, PreviewKind, PreviewTabs, classify_preview_kind};

    #[test]
    fn classifies_reference_viewer_matrix() {
        assert_eq!(classify_preview_kind("README.md"), PreviewKind::Markdown);
        assert_eq!(classify_preview_kind("main.rs"), PreviewKind::Code);
        assert_eq!(classify_preview_kind("report.html"), PreviewKind::Html);
        assert_eq!(classify_preview_kind("photo.png"), PreviewKind::Image);
        assert_eq!(classify_preview_kind("manual.pdf"), PreviewKind::Pdf);
        assert_eq!(classify_preview_kind("demo.mp4"), PreviewKind::Video);
        assert_eq!(
            classify_preview_kind("Screen Recording.mov"),
            PreviewKind::Video
        );
        assert_eq!(classify_preview_kind("clip.webm"), PreviewKind::Video);
        assert_eq!(classify_preview_kind("data.csv"), PreviewKind::Data);
        assert_eq!(classify_preview_kind("book.xlsx"), PreviewKind::Data);
        assert_eq!(
            classify_preview_kind("archive.zip"),
            PreviewKind::Unsupported
        );
    }

    #[test]
    fn tabs_deduplicate_select_and_close_like_the_reference() {
        let mut tabs = PreviewTabs::default();
        tabs.open("ctx", "README.md");
        tabs.open("ctx", "src/main.rs");
        tabs.open("ctx", "README.md");
        assert_eq!(tabs.paths("ctx"), ["README.md", "src/main.rs"]);
        assert_eq!(tabs.active_path("ctx"), Some("README.md"));

        tabs.close("ctx", "README.md");
        assert_eq!(tabs.active_path("ctx"), Some("src/main.rs"));
        tabs.close("ctx", "src/main.rs");
        assert_eq!(tabs.active_path("ctx"), None);
    }

    #[test]
    fn tabs_are_isolated_per_project_context() {
        let mut tabs = PreviewTabs::default();
        tabs.open("one", "README.md");
        tabs.open("two", "Cargo.toml");
        assert_eq!(tabs.active_path("one"), Some("README.md"));
        assert_eq!(tabs.active_path("two"), Some("Cargo.toml"));
    }

    #[test]
    fn display_mode_toggles_between_side_peek_and_full_page() {
        assert_eq!(
            PreviewDisplayMode::SidePeek.toggled(),
            PreviewDisplayMode::FullPage
        );
        assert_eq!(
            PreviewDisplayMode::FullPage.toggled(),
            PreviewDisplayMode::SidePeek
        );
    }
}
