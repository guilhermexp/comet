//! Embedded icon assets + the gpui [`AssetSource`] that serves them.
//!
//! The set mirrors the original zeron's icon usage exactly:
//! - Most glyphs come from the **Solar Icons** set (Linear weight) by 480 Design,
//!   the same set the Electron app used via `@solar-icons/react`. Solar Icons is
//!   licensed under CC BY 4.0 (https://creativecommons.org/licenses/by/4.0/);
//!   attribution: "Solar Icons by 480 Design".
//! - The terminal tab glyphs (`terminal`, `plus`, `close`) and the stop square
//!   are ports of the hand-drawn inline SVGs in zeron's `terminal-panel.tsx` /
//!   `composer-actions.tsx`.
//! - The harness brand marks (`claude-mark`, `openai-mark`, `cursor-mark`) are
//!   ports of zeron's `icons.tsx`. gpui tints SVGs with the text color, so the
//!   Claude mark's brand orange is applied at the call site ([`CLAUDE_BRAND`]).
//!
//! Icons render via [`icon`]: `icon(icons::PAPERCLIP).size(px(16.)).text_color(…)`.

use std::borrow::Cow;

use std::sync::Arc;

use gpui::{AssetSource, Hsla, Image, ImageFormat, Result, SharedString, Styled as _, Svg, svg};

mod material_file_icon_assets {
    include!(concat!(env!("OUT_DIR"), "/material_file_icon_assets.rs"));
}

macro_rules! icon_assets {
    ($(($const_name:ident, $path:literal)),+ $(,)?) => {
        $(pub const $const_name: &str = concat!("icons/", $path, ".svg");)+

        /// Serves the embedded icons to gpui's SVG renderer.
        pub struct Assets;

        impl AssetSource for Assets {
            fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
                if let Some(bytes) = material_file_icon_assets::load(path) {
                    return Ok(Some(Cow::Borrowed(bytes)));
                }
                Ok(match path {
                    $(concat!("icons/", $path, ".svg") => Some(Cow::Borrowed(
                        include_bytes!(concat!("../assets/icons/", $path, ".svg")).as_slice(),
                    )),)+
                    _ => None,
                })
            }

            fn list(&self, path: &str) -> Result<Vec<SharedString>> {
                let all = [$(concat!("icons/", $path, ".svg")),+];
                Ok(all
                    .iter()
                    .filter(|p| p.starts_with(path))
                    .map(|p| SharedString::from(*p))
                    .collect())
            }
        }
    };
}

icon_assets![
    // Solar Icons (Linear), CC BY 4.0 — 480 Design.
    (MONITOR, "monitor"),
    (LAPTOP, "laptop"),
    (PEN_NEW_SQUARE, "pen-new-square"),
    (SORT_VERTICAL, "sort-vertical"),
    (LIST, "list"),
    (FOLDER_WITH_FILES, "folder-with-files"),
    (FOLDER, "folder"),
    // Hand-drawn git-branch glyph in the Solar Linear style (like the
    // terminal/plus/return ports) — the set has no branch icon.
    (GIT_BRANCH, "git-branch"),
    (DIFF, "diff"),
    // Compact history-ref glyphs, drawn in the same linear style.
    (CLOUD, "cloud"),
    (TAG, "tag"),
    (SIDEBAR_MINIMALISTIC, "sidebar-minimalistic"),
    // Mirrored variant (zeron window-controls.tsx `-scale-x-100`): the LEFT
    // sidebar toggle shows the panel line on the left; gpui divs have no
    // scale transform at the pinned rev, so the flip is baked into the asset.
    (SIDEBAR_MINIMALISTIC_LEFT, "sidebar-minimalistic-left"),
    (KEY_MINIMALISTIC, "key-minimalistic"),
    (KEYBOARD, "keyboard"),
    (ARROW_LEFT, "arrow-left"),
    (ARROW_RIGHT, "arrow-right"),
    (ARROW_UP, "arrow-up"),
    // arrow-up mirrored (like the sidebar flip) — the Solar Linear set here
    // has no plain arrow-down.
    (ARROW_DOWN, "arrow-down"),
    // Hand-drawn return/enter arrow in the Solar Linear style (like the
    // terminal/plus/close ports) — the set has no return glyph.
    (RETURN, "return"),
    (ALT_ARROW_DOWN, "alt-arrow-down"),
    // Hand-drawn expand/maximize arrows in the Solar Linear style (like the
    // terminal/plus/return ports) — the set has no expand glyph.
    (EXPAND_ARROWS, "expand-arrows"),
    // Hand-drawn fold-all chevrons, drawn as a family with EXPAND_ARROWS
    // (same stroke, caps, 90° joints) — Solar has no unfold-less either.
    (FOLD_VERTICAL, "fold-vertical"),
    (ALT_ARROW_LEFT, "alt-arrow-left"),
    (ALT_ARROW_RIGHT, "alt-arrow-right"),
    (SMARTPHONE, "smartphone"),
    (ARCHIVE_UP_MINIMALISTIC, "archive-up-minimalistic"),
    (REFRESH, "refresh"),
    (RESTART, "restart"),
    (ADD_CIRCLE, "add-circle"),
    (TUNING, "tuning"),
    (PAPERCLIP, "paperclip"),
    (PEN, "pen"),
    (ARCHIVE_MINIMALISTIC, "archive-minimalistic"),
    (TRASH_BIN_MINIMALISTIC, "trash-bin-minimalistic"),
    (SETTINGS_MINIMALISTIC, "settings-minimalistic"),
    (LOGOUT_2, "logout-2"),
    (MAGNIFER, "magnifer"),
    (COMMAND, "command"),
    (DOCUMENT, "document"),
    (DOCUMENT_ADD, "document-add"),
    (GLOBAL, "global"),
    (CHECKLIST, "checklist"),
    (WIDGET, "widget"),
    (WIFI_OFF, "wifi-off"),
    (CLOSE_CIRCLE, "close-circle"),
    // Hand-drawn info glyph in the Solar Linear style (like the terminal/
    // plus/return ports) — the embedded set has no info-circle.
    (INFO_CIRCLE, "info-circle"),
    (DETAILS_CHEVRONS_RIGHT, "details-chevrons-right"),
    (DETAILS_EYE, "details-eye"),
    (DETAILS_EYE_OFF, "details-eye-off"),
    (DETAILS_BOX, "details-box"),
    (DETAILS_GAUGE, "details-gauge"),
    (DETAILS_FILES, "details-files"),
    (DANGER_TRIANGLE, "danger-triangle"),
    (CHAT_ROUND_LINE, "chat-round-line"),
    // Hand-drawn bot head (antenna + eyes + ears) in the Solar Linear style
    // — the embedded set has no bot/robot glyph. Subagent tabs.
    (BOT, "bot"),
    // Hand-drawn bell + speaker in the Solar Linear style (like the terminal/
    // plus/return ports) — the embedded set has neither.
    (BELL, "bell"),
    (VOLUME_LOUD, "volume-loud"),
    // Hand-drawn zeron glyphs (terminal-panel.tsx / composer-actions.tsx /
    // menu-check.tsx / logo.tsx).
    (TERMINAL, "terminal"),
    (PLUS, "plus"),
    (CLOSE, "close"),
    // Hand-drawn Linux caption glyphs (minimize dash, maximize square,
    // restore stacked squares) in the same style as `close` — drawn for the
    // client-side-decoration window controls; no system glyph font exists on
    // Linux the way Segoe Fluent Icons does on Windows.
    (WINDOW_MINIMIZE, "window-minimize"),
    (WINDOW_MAXIMIZE, "window-maximize"),
    (WINDOW_RESTORE, "window-restore"),
    (STOP, "stop"),
    (CHECK, "check"),
    (COPY, "copy"),
    // Hand-drawn star pair in the Solar Linear style (like the terminal/
    // plus/return ports) — outline for the favorite affordance, bold for the
    // favorited state and the picker's favorites rail tab.
    (STAR, "star"),
    (STAR_BOLD, "star-bold"),
    (ZERON_LOGO, "zeron-logo"),
    // Harness brand marks (icons.tsx).
    (CLAUDE_MARK, "claude-mark"),
    (OPENAI_MARK, "openai-mark"),
    (CURSOR_MARK, "cursor-mark"),
    (GROK_MARK, "grok-mark"),
    (HERMES_MARK, "hermes-mark"),
    (PI_MARK, "pi-mark"),
    (OPENCODE_MARK, "opencode-mark"),
    // Unpeel runtime package marks. These are copied from
    // `third_party/unpeel/runtimes/*/assets/icon.svg` so packaged builds do
    // not depend on the source submodule at runtime.
    (WORKER_AMP, "workers/amp"),
    (WORKER_CLAUDE, "workers/claude"),
    (WORKER_CLINE, "workers/cline"),
    (WORKER_CODEX, "workers/codex"),
    (WORKER_CURSOR, "workers/cursor-agent"),
    (WORKER_GEMINI, "workers/gemini"),
    (WORKER_GROK, "workers/grok"),
    (WORKER_KIMI, "workers/kimi"),
    (WORKER_KIRO, "workers/kiro"),
    (WORKER_MUSE, "workers/muse-code"),
    (WORKER_OPENCODE, "workers/opencode"),
    (WORKER_OMP, "workers/omp"),
    (WORKER_PI, "workers/pi"),
    (WORKER_PRIME_AGENT, "workers/prime-agent"),
    // Unpeel's authored provider catalog has one deliberate fallback:
    // GitHub Copilot uses the shared generic-agent SVG.
    (WORKER_GENERIC_AGENT, "workers/generic-agent"),
    // Exact sidebar chrome carried from Unpeel's ChromeIcons.swift.
    (WORKER_FOLDER_CLOSED, "workers/chrome-folder-closed"),
    (WORKER_FOLDER_OPEN, "workers/chrome-folder-open"),
    (WORKER_FOLDER_SIMPLE, "workers/chrome-folder-simple"),
    (WORKER_BRANCH, "workers/chrome-branch"),
    (WORKER_GIT_BRANCH, "workers/chrome-git-branch"),
    (WORKER_PIN, "workers/chrome-pin"),
    (WORKER_PUSH_PIN, "workers/chrome-push-pin"),
    (WORKER_SETTINGS, "workers/chrome-settings"),
    (WORKER_ADD_PROJECT_PLUS, "workers/chrome-add-project-plus"),
    (WORKER_PLUS, "workers/chrome-plus"),
    (WORKER_COLLAPSE_ALL, "workers/chrome-collapse-all"),
    (WORKER_DRAG_HANDLE, "workers/chrome-drag-handle"),
    (WORKER_OPEN_CODE, "workers/chrome-open-code"),
    (WORKER_GALLERY, "workers/chrome-gallery"),
    (WORKER_UNPEEL_LOGO, "workers/unpeel-logo"),
];

/// The Claude mark's brand orange (`#D97757`) — zeron keeps it even on the
/// monochrome surface.
pub fn claude_brand() -> Hsla {
    gpui::rgb(0xD97757).into()
}

/// An icon element for an embedded asset path. Size and colour are set by the
/// caller (`.size(..)`, `.text_color(..)`), matching the web app's
/// `[&_svg]:size-4` idiom.
pub fn icon(path: impl Into<SharedString>) -> Svg {
    svg().path(path.into()).flex_none()
}

pub fn material_file_icon_image(path: &str) -> Option<Arc<Image>> {
    let bytes = material_file_icon_assets::load(path)?;
    Some(Arc::new(Image::from_bytes(
        ImageFormat::Svg,
        bytes.to_vec(),
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_registered_icon_loads_and_parses() {
        let assets = Assets;
        for path in assets.list("icons/").unwrap() {
            let bytes = assets
                .load(&path)
                .unwrap()
                .unwrap_or_else(|| panic!("missing asset {path}"));
            let text = std::str::from_utf8(&bytes).expect("icon svg is utf-8");
            assert!(text.contains("<svg"), "{path} is not an svg");
            assert!(text.contains("viewBox"), "{path} lacks a viewBox");
        }
    }

    #[test]
    fn unknown_paths_are_none() {
        assert!(Assets.load("icons/nope.svg").unwrap().is_none());
    }

    #[test]
    fn material_file_icon_assets_are_embedded() {
        for path in [
            "file-icons/readme.svg",
            "file-icons/nodejs.svg",
            "file-icons/rust.svg",
            "file-icons/react_ts.svg",
            "file-icons/folder-src.svg",
            "file-icons/folder-src-open.svg",
        ] {
            let bytes = Assets
                .load(path)
                .unwrap()
                .unwrap_or_else(|| panic!("missing material icon {path}"));
            assert!(std::str::from_utf8(&bytes).unwrap().contains("<svg"));
        }
    }

    #[test]
    fn list_filters_by_prefix() {
        assert!(!Assets.list("icons/").unwrap().is_empty());
        assert!(Assets.list("fonts/").unwrap().is_empty());
    }
}
