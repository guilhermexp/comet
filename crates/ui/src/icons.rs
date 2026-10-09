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

use gpui::{
    AssetSource, Div, Hsla, Image, ImageFormat, ParentElement as _, Result, SharedString,
    Styled as _, Svg, div, px, svg,
};

const SVG_SANS_FONT: &str = "fonts/ibm-plex-sans/IBMPlexSans-Regular.ttf";
const SVG_MONO_FONT: &str = "fonts/lilex/Lilex-Regular.ttf";

mod material_file_icon_assets {
    include!(concat!(env!("OUT_DIR"), "/material_file_icon_assets.rs"));
}

mod blobatar_subagent_avatar_assets {
    include!(concat!(
        env!("OUT_DIR"),
        "/blobatar_subagent_avatar_assets.rs"
    ));
}

macro_rules! icon_assets {
    ($(($const_name:ident, $path:literal)),+ $(,)?) => {
        $(pub const $const_name: &str = concat!("icons/", $path, ".svg");)+

        /// Serves the embedded control icons to gpui's SVG renderer.
        struct ControlAssets;

        impl AssetSource for ControlAssets {
            fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
                match path {
                    SVG_SANS_FONT => return Ok(Some(Cow::Borrowed(crate::typography::GEIST[0]))),
                    SVG_MONO_FONT => {
                        return Ok(Some(Cow::Borrowed(crate::typography::GEIST_MONO[0])));
                    }
                    _ => {}
                }
                if let Some(bytes) = material_file_icon_assets::load(path) {
                    return Ok(Some(Cow::Borrowed(bytes)));
                }
                if let Some(bytes) = blobatar_subagent_avatar_assets::load(path) {
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
                    .into_iter()
                    .chain([SVG_SANS_FONT, SVG_MONO_FONT])
                    .chain(blobatar_subagent_avatar_assets::PATHS.iter().copied())
                    .filter(|p| p.starts_with(path))
                    .map(SharedString::from)
                    .collect())
            }
        }
    };
}

icon_assets![
    (PROJECT_DEFAULT, "project-default"),
    (REMOTE_SERVER, "remote-server"),
    // Service-tier bolt, drawn in the toolbar family's linear weight; the
    // filled twin marks fast mode on.
    (FAST_TIER, "fast-tier"),
    (FAST_TIER_BOLD, "fast-tier-bold"),
    // Solar Icons (Linear), CC BY 4.0 — 480 Design.
    (MONITOR, "monitor"),
    (SUN, "sun"),
    (MOON, "moon"),
    // Browser globe, drawn in the same linear weight as the toolbar family.
    (GLOBE, "globe"),
    (LAPTOP, "laptop"),
    (PEN_NEW_SQUARE, "pen-new-square"),
    (SORT, "sort"),
    (MORE_HORIZONTAL, "more-horizontal"),
    (SORT_VERTICAL, "sort-vertical"),
    // Compact six-dot grip used to reorder queued prompts.
    (DRAG_HANDLE, "drag-handle"),
    // Original queue-only line family: 24px canvas, 1.5px round strokes and
    // medium-radius geometry. Kept separate so the queue can adopt the visual
    // language of the supplied Central Icons reference without changing
    // shared application glyphs or copying third-party artwork.
    (QUEUE_DRAG_HANDLE, "queue-drag-handle"),
    (QUEUE_SEND, "queue-send"),
    (QUEUE_CHECK, "queue-check"),
    (QUEUE_CLOSE, "queue-close"),
    (QUEUE_PAPERCLIP, "queue-paperclip"),
    (CLOCK_CIRCLE, "clock-circle"),
    (CALENDAR, "calendar"),
    (LIST, "list"),
    (FOLDER_WITH_FILES, "folder-with-files"),
    // Original tree glyph with compact nodes for the independent Files panel.
    (FILE_TREE, "file-tree"),
    // Hand-drawn floppy disk in the Solar Linear style. Workspace editor save.
    (FLOPPY_DISK, "floppy-disk"),
    // Zeron Icons (icons.zeron.sh): 24px canvas, 1.75px round strokes. The
    // git family shares rails at x=6/18 and 2.25-radius nodes; the carets
    // and arrows are the site's one path pre-rotated per direction.
    (FOLDER, "folder"),
    (FORK, "fork"),
    (GIT_BRANCH, "git-branch"),
    (DIFF, "diff"),
    // Provider-neutral pull-request glyph, drawn in the same linear family.
    (PULL_REQUEST, "pull-request"),
    (WORKTREE, "worktree"),
    (PLUS, "plus"),
    (ARROW_LEFT, "arrow-left"),
    (ARROW_RIGHT, "arrow-right"),
    (ALT_ARROW_DOWN, "alt-arrow-down"),
    (ALT_ARROW_UP, "alt-arrow-up"),
    (ALT_ARROW_LEFT, "alt-arrow-left"),
    (ALT_ARROW_RIGHT, "alt-arrow-right"),
    // Compact history-ref glyphs, drawn in the same linear style.
    (CLOUD, "cloud"),
    (TAG, "tag"),
    (KEY_MINIMALISTIC, "key-minimalistic"),
    (KEYBOARD, "keyboard"),
    (ARROW_UP, "arrow-up"),
    // arrow-up mirrored (like the sidebar flip) — the Solar Linear set here
    // has no plain arrow-down.
    (ARROW_DOWN, "arrow-down"),
    // arrow-up rotated 45° — the "opens elsewhere" glyph on spawn chips;
    // the set has no diagonal arrow.
    (ARROW_UP_RIGHT, "arrow-up-right"),
    // Hand-drawn return/enter arrow in the Solar Linear style (like the
    // terminal/plus/close ports) — the set has no return glyph.
    (RETURN, "return"),
    // Hand-drawn expand/maximize arrows in the Solar Linear style (like the
    // terminal/plus/return ports) — the set has no expand glyph.
    (EXPAND_ARROWS, "expand-arrows"),
    // Inward-pointing companion used to restore an expanded pane.
    (COLLAPSE_ARROWS, "collapse-arrows"),
    // Hand-drawn fold-all chevrons, drawn as a family with EXPAND_ARROWS
    // (same stroke, caps, 90° joints) — Solar has no unfold-less either.
    (FOLD_VERTICAL, "fold-vertical"),
    // The changes pane's unified/split toggle: a rounded frame halved by a
    // centre rule (Solar Linear weight).
    (SPLIT_COLUMNS, "split-columns"),
    // Long-line wrapping toggle shared by changes and agent Markdown fences.
    (WRAP_TEXT, "wrap-text"),
    (SMARTPHONE, "smartphone"),
    (ARCHIVE_UP_MINIMALISTIC, "archive-up-minimalistic"),
    (REFRESH, "refresh"),
    (RESTART, "restart"),
    (ADD_CIRCLE, "add-circle"),
    (TUNING, "tuning"),
    (EYE, "eye"),
    (EYE_CLOSED, "eye-closed"),
    (PAPERCLIP, "paperclip"),
    (MICROPHONE, "microphone"),
    // Hand-drawn pushpin in the Solar Linear style for local sidebar pins.
    (PIN, "pin"),
    (PEN, "pen"),
    (ARCHIVE_MINIMALISTIC, "archive-minimalistic"),
    (TRASH_BIN_MINIMALISTIC, "trash-bin-minimalistic"),
    // Shared settings glyph: user-supplied horizontal sliders.
    (SETTINGS, "settings"),
    (SETTINGS_MINIMALISTIC, "settings-minimalistic"),
    (SIDEBAR_MINIMALISTIC, "sidebar-minimalistic"),
    (LOGOUT_2, "logout-2"),
    (MAGNIFER, "magnifer"),
    // Compact magnifier with a distinct handle, matching the linear icon family.
    (PALETTE_SEARCH, "palette-search"),
    (COMMAND, "command"),
    (DOCUMENT, "document"),
    (DOCUMENT_ADD, "document-add"),
    // File-kind glyphs, drawn in the same linear family for transcript badges.
    (FILE_CODE, "file-code"),
    (FILE_STYLE, "file-style"),
    (FILE_DATA, "file-data"),
    (FILE_MARKDOWN, "file-markdown"),
    (FILE_IMAGE, "file-image"),
    // A framed picture: the icon of `Image N` chips.
    (GALLERY, "gallery"),
    (GLOBAL, "global"),
    (CHECKLIST, "checklist"),
    (WIDGET, "widget"),
    (MAGIC_STICK_3, "magic-stick-3"),
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
    // Hand-drawn microphone pair in the Solar Linear style — voice controls.
    (MICROPHONE_OFF, "microphone-off"),
    (PHONE_HANG_UP, "phone-hang-up"),
    // Hand-drawn zeron glyphs (terminal-panel.tsx / composer-actions.tsx /
    // menu-check.tsx / logo.tsx).
    (TERMINAL, "terminal"),
    // `plus` with the vertical stroke removed — the zoom-out half of a zoom
    // pair has to read as its sibling's opposite, and the set has no minus.
    (MINUS, "minus"),
    (CLOSE, "close"),
    // Hand-drawn Linux caption glyphs (minimize dash, maximize square,
    // restore stacked squares) in the same style as `close` — drawn for the
    // client-side-decoration window controls; no system glyph font exists on
    // Linux the way Segoe Fluent Icons does on Windows.
    (WINDOW_MINIMIZE, "window-minimize"),
    (WINDOW_MAXIMIZE, "window-maximize"),
    (WINDOW_RESTORE, "window-restore"),
    // Hand-drawn hard-drive + home glyphs in the Solar Linear style (like the
    // terminal/plus/return ports) — drawn for the add-space palette's
    // Locations rail; the set has neither.
    (HARD_DRIVE, "hard-drive"),
    (HOME, "home"),
    (STOP, "stop"),
    (CHECK, "check"),
    (COPY, "copy"),
    // Project Action icon family (Solar Linear-compatible strokes).
    (ACTION_PLAY, "action-play"),
    (ACTION_TEST, "action-test"),
    (ACTION_LINT, "action-lint"),
    (ACTION_CONFIGURE, "action-configure"),
    (ACTION_BUILD, "action-build"),
    (ACTION_DEBUG, "action-debug"),
    // Hand-drawn star pair in the Solar Linear style (like the terminal/
    // plus/return ports) — outline for the favorite affordance, bold for the
    // favorited state and the picker's favorites rail tab.
    (STAR, "star"),
    (STAR_BOLD, "star-bold"),
    // Dedicated singular four-point sparkle for reasoning/Thought headers.
    (THOUGHT_SPARKLE, "thought-sparkle"),
    (ZERON_LOGO, "zeron-logo"),
    // Harness brand marks (icons.tsx).
    (CLAUDE_MARK, "claude-mark"),
    (OPENAI_MARK, "openai-mark"),
    (CURSOR_MARK, "cursor-mark"),
    (DEVIN_MARK, "devin-mark"),
    (GROK_MARK, "grok-mark"),
    (HERMES_MARK, "hermes-mark"),
    (PI_MARK, "pi-mark"),
    (OPENCODE_MARK, "opencode-mark"),
    (ANTIGRAVITY, "antigravity"),
    // Link-source marks for the GitHub/YouTube URL chips (official marks,
    // single-path so gpui's currentColor tint carries the chip's ink).
    (GITHUB_MARK, "github-mark"),
    (YOUTUBE_MARK, "youtube-mark"),
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
    (ANTIGRAVITY_MARK, "antigravity-mark"),
];

/// Details-card glyph for the chat-scoped workflow/subagent/worker projection.
/// The existing widget asset already matches the connected-node visual language.
pub const DETAILS_WORKERS: &str = WIDGET;
pub const WORKER_ANTIGRAVITY: &str = ANTIGRAVITY;

/// Serves the compact control-icon set (with the fork's Material file icons,
/// SVG font aliases and Blobatar avatars) first, then upstream's file-identity
/// icon theme, through the single asset source registered at app startup.
pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if let Some(asset) = ControlAssets.load(path)? {
            return Ok(Some(asset));
        }
        crate::file_icons::Assets.load(path)
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut assets = ControlAssets.list(path)?;
        assets.extend(crate::file_icons::Assets.list(path)?);
        Ok(assets)
    }
}

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

/// The Zeron Icons sidebar glyph (icons.zeron.sh "Sidebar" / "Right
/// sidebar"): a rounded frame holding a panel whose width morphs 5.5 → 1.75
/// as the sidebar closes. gpui SVGs are static, so it is drawn from quads in
/// the source's 24-unit space, scaled to `size`. `open` is the morph progress
/// (1 = open; drive it with [`crate::motion::state_t`]); `right` mirrors it.
pub fn sidebar_glyph(open: f32, right: bool, size: f32, color: Hsla) -> Div {
    let s = size / 24.0;
    // `<rect x=3 y=4 w=18 h=16 rx=4 stroke-width=1.75>`: the centered stroke
    // grows the box by half the stroke on each side.
    let stroke = 1.75;
    let frame = div()
        .absolute()
        .left(px((3.0 - stroke / 2.0) * s))
        .top(px((4.0 - stroke / 2.0) * s))
        .w(px((18.0 + stroke) * s))
        .h(px((16.0 + stroke) * s))
        .rounded(px((4.0 + stroke / 2.0) * s))
        .border(px(stroke * s))
        .border_color(color);
    // `<rect class=zi-panel x=6.5 y=7.5 w=5.5 h=9 rx=.875>`; closed: w=1.75.
    let inset = px(6.5 * s);
    let panel = div()
        .absolute()
        .top(px(7.5 * s))
        .w(px(crate::motion::lerp(1.75, 5.5, open.clamp(0.0, 1.0)) * s))
        .h(px(9.0 * s))
        .rounded(px(0.875 * s))
        .bg(color);
    let panel = if right {
        panel.right(inset)
    } else {
        panel.left(inset)
    };
    div()
        .relative()
        .flex_none()
        .size(px(size))
        .child(frame)
        .child(panel)
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
    fn blobatar_subagent_avatar_assets_include_every_variant() {
        let assets = Assets;
        let paths = assets.list("icons/subagents/blobatar/").unwrap();
        assert_eq!(paths.len(), 28);
        for path in paths {
            let bytes = assets.load(&path).unwrap().expect("embedded avatar");
            let svg = std::str::from_utf8(&bytes).expect("avatar svg is utf-8");
            assert!(svg.contains("<svg"), "{path}");
            assert!(svg.contains("viewBox"), "{path}");
            Image::from_bytes(ImageFormat::Svg, bytes.to_vec())
                .to_image_data(gpui::SvgRenderer::new(Arc::new(())))
                .unwrap_or_else(|error| panic!("avatar {path} failed GPUI rendering: {error}"));
        }
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
        assert_eq!(
            Assets.list("fonts/").unwrap(),
            vec![
                SharedString::from("fonts/ibm-plex-sans/IBMPlexSans-Regular.ttf"),
                SharedString::from("fonts/lilex/Lilex-Regular.ttf"),
            ]
        );
    }

    #[test]
    fn svg_renderer_font_aliases_load_embedded_fonts() {
        for path in [
            "fonts/ibm-plex-sans/IBMPlexSans-Regular.ttf",
            "fonts/lilex/Lilex-Regular.ttf",
        ] {
            let bytes = Assets
                .load(path)
                .unwrap()
                .unwrap_or_else(|| panic!("missing SVG renderer font alias {path}"));
            assert!(!bytes.is_empty(), "empty SVG renderer font alias {path}");
        }
    }
}
