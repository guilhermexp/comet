use gpui::{Context, IntoElement, Render, SharedString, Window, div, prelude::*, px};

use crate::theme::Theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileMenuKind {
    Root,
    Entry,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileCheckoutAccess {
    Local,
    Remote,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileClipboardPresence {
    Empty,
    Occupied,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileMenuItem {
    NewFile,
    NewFolder,
    Cut,
    Copy,
    Paste,
    Duplicate,
    CopyPath,
    CopyRelativePath,
    Rename,
    Delete,
    OpenInTerminal,
    RevealInFinder,
}

impl FileMenuItem {
    pub fn label(self) -> &'static str {
        match self {
            Self::NewFile => "New File",
            Self::NewFolder => "New Folder",
            Self::Cut => "Cut",
            Self::Copy => "Copy",
            Self::Paste => "Paste",
            Self::Duplicate => "Duplicate",
            Self::CopyPath => "Copy Path",
            Self::CopyRelativePath => "Copy Relative Path",
            Self::Rename => "Rename",
            Self::Delete => "Delete",
            Self::OpenInTerminal => "Open in Terminal",
            Self::RevealInFinder => "Reveal in Finder",
        }
    }

    pub fn shortcut(self) -> Option<&'static str> {
        match self {
            Self::Cut => Some("⌘X"),
            Self::Copy => Some("⌘C"),
            Self::Paste => Some("⌘V"),
            Self::Rename => Some("F2"),
            Self::Delete => Some("⌫"),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileMenuEntry {
    pub item: FileMenuItem,
    pub enabled: bool,
}

pub fn file_menu_items(
    kind: FileMenuKind,
    access: FileCheckoutAccess,
    clipboard: FileClipboardPresence,
) -> Vec<FileMenuEntry> {
    let root = kind == FileMenuKind::Root;
    let paste = clipboard == FileClipboardPresence::Occupied;
    let mut items = vec![
        FileMenuEntry {
            item: FileMenuItem::NewFile,
            enabled: true,
        },
        FileMenuEntry {
            item: FileMenuItem::NewFolder,
            enabled: true,
        },
        FileMenuEntry {
            item: FileMenuItem::Cut,
            enabled: !root,
        },
        FileMenuEntry {
            item: FileMenuItem::Copy,
            enabled: !root,
        },
        FileMenuEntry {
            item: FileMenuItem::Paste,
            enabled: paste,
        },
        FileMenuEntry {
            item: FileMenuItem::Duplicate,
            enabled: !root,
        },
        FileMenuEntry {
            item: FileMenuItem::CopyPath,
            enabled: true,
        },
        FileMenuEntry {
            item: FileMenuItem::CopyRelativePath,
            enabled: !root,
        },
        FileMenuEntry {
            item: FileMenuItem::Rename,
            enabled: !root,
        },
        FileMenuEntry {
            item: FileMenuItem::Delete,
            enabled: !root,
        },
    ];
    if access == FileCheckoutAccess::Local {
        items.push(FileMenuEntry {
            item: FileMenuItem::OpenInTerminal,
            enabled: true,
        });
        items.push(FileMenuEntry {
            item: FileMenuItem::RevealInFinder,
            enabled: true,
        });
    }
    items
}

pub struct FileActionTooltip {
    pub label: SharedString,
}

impl Render for FileActionTooltip {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::of(cx);
        div()
            .px(px(8.0))
            .py(px(5.0))
            .rounded(px(6.0))
            .bg(theme.composer_glass_bg())
            .border_1()
            .border_color(theme.border)
            .text_size(px(11.0))
            .text_color(theme.text)
            .child(self.label.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_disables_rename_delete_and_cut() {
        let items = file_menu_items(
            FileMenuKind::Root,
            FileCheckoutAccess::Local,
            FileClipboardPresence::Empty,
        );
        for item in [
            FileMenuItem::Rename,
            FileMenuItem::Delete,
            FileMenuItem::Cut,
        ] {
            let entry = items.iter().find(|entry| entry.item == item).unwrap();
            assert!(!entry.enabled, "{item:?} should be unavailable on root");
        }
        assert!(
            items
                .iter()
                .any(|entry| entry.item == FileMenuItem::RevealInFinder)
        );
    }

    #[test]
    fn remote_checkout_omits_finder_and_terminal() {
        let items = file_menu_items(
            FileMenuKind::Entry,
            FileCheckoutAccess::Remote,
            FileClipboardPresence::Occupied,
        );
        assert!(
            !items
                .iter()
                .any(|entry| entry.item == FileMenuItem::RevealInFinder
                    || entry.item == FileMenuItem::OpenInTerminal)
        );
        assert!(
            items
                .iter()
                .any(|entry| entry.item == FileMenuItem::Copy && entry.enabled)
        );
        assert!(
            items
                .iter()
                .any(|entry| entry.item == FileMenuItem::Paste && entry.enabled)
        );
    }
}
