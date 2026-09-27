use std::collections::HashMap;

pub use zeron_markdown::file_path::{
    PreviewKind, classify_preview_kind, is_file_path_candidate, strip_line_col,
};

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

pub fn expand_tilde(path: &str) -> std::borrow::Cow<'_, str> {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            let mut buf = std::path::PathBuf::from(home);
            buf.push(rest);
            return std::borrow::Cow::Owned(buf.to_string_lossy().into_owned());
        }
    } else if path == "~" {
        if let Some(home) = std::env::var_os("HOME") {
            return std::borrow::Cow::Owned(home.to_string_lossy().into_owned());
        }
    }
    std::borrow::Cow::Borrowed(path)
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
