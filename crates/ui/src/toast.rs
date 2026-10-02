//! Floating toast notification system anchored in the bottom-right of the window.
//!
//! Provides transient, floating notification cards with titles, optional
//! descriptions and auto-dismissal timers.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use gpui::{AnyElement, Context, SharedString, div, prelude::*, px};

use crate::icons;
use crate::theme::Theme;

static NEXT_TOAST_ID: AtomicU64 = AtomicU64::new(1);

fn next_toast_id() -> u64 {
    NEXT_TOAST_ID.fetch_add(1, Ordering::Relaxed)
}

#[derive(Clone, Debug, PartialEq)]
pub enum ToastKind {
    Info,
    Success,
    Warning,
    Error,
}

#[derive(Clone, Debug)]
pub struct Toast {
    pub id: u64,
    pub kind: ToastKind,
    pub title: SharedString,
    pub description: Option<SharedString>,
    pub created_at: Instant,
    pub duration: Option<Duration>,
}

impl Toast {
    pub fn new(title: impl Into<SharedString>) -> Self {
        Self {
            id: next_toast_id(),
            kind: ToastKind::Info,
            title: title.into(),
            description: None,
            created_at: Instant::now(),
            duration: Some(Duration::from_secs(5)),
        }
    }

    pub fn is_expired(&self, now: Instant) -> bool {
        if let Some(duration) = self.duration {
            now.saturating_duration_since(self.created_at) >= duration
        } else {
            false
        }
    }

    pub fn success(title: impl Into<SharedString>) -> Self {
        Self {
            id: next_toast_id(),
            kind: ToastKind::Success,
            title: title.into(),
            description: None,
            created_at: Instant::now(),
            duration: Some(Duration::from_secs(4)),
        }
    }

    pub fn error(title: impl Into<SharedString>) -> Self {
        Self {
            id: next_toast_id(),
            kind: ToastKind::Error,
            title: title.into(),
            description: None,
            created_at: Instant::now(),
            duration: Some(Duration::from_secs(5)),
        }
    }

    pub fn info(title: impl Into<SharedString>) -> Self {
        Self {
            id: next_toast_id(),
            kind: ToastKind::Info,
            title: title.into(),
            description: None,
            created_at: Instant::now(),
            duration: Some(Duration::from_secs(4)),
        }
    }
}

/// Shared with the composer pill / ContextUsageTooltip: thinned glass fill.
pub(crate) fn toast_card_background(theme: &Theme) -> gpui::Hsla {
    theme.composer_glass_bg()
}

/// Render a single toast card matching Orchestrator.dev's styling.
pub fn render_toast_card<S: 'static>(
    toast: &Toast,
    theme: &Theme,
    on_dismiss: impl Fn(&mut S, u64, &mut gpui::Window, &mut Context<S>) + 'static + Copy,
    cx: &mut Context<S>,
) -> AnyElement {
    let toast_id = toast.id;

    let leading_icon = match toast.kind {
        ToastKind::Error => Some((icons::CLOSE_CIRCLE, theme.danger)),
        ToastKind::Warning => Some((icons::INFO_CIRCLE, theme.warning)),
        ToastKind::Success => Some((icons::CHECK, theme.success)),
        _ => None,
    };

    // Same plate as ContextUsageTooltip: frost 10/16, 10px radius, shared
    // composer glass fill and hairline. Semantic tone lives on the icon,
    // not on a tinted border — the green success edge read as an alert.
    let mut card = div()
        .id(("toast-item", toast.id))
        .w(px(320.0))
        .p(px(14.0))
        .rounded(px(10.0))
        .border_1()
        .border_color(theme.border)
        .bg(toast_card_background(theme))
        .when(!theme.is_frost(), |el| el.shadow_md())
        .flex()
        .flex_col()
        .gap(px(6.0));

    // Header row: Title + dismiss button
    let mut header = div().flex().items_start().justify_between().gap(px(8.0));

    let mut title_content = div().flex().items_center().gap(px(6.0));
    if let Some((ico, color)) = leading_icon {
        title_content =
            title_content.child(crate::icons::icon(ico).size(px(13.0)).text_color(color));
    }
    title_content = title_content.child(
        div()
            .text_size(px(13.0))
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(theme.text)
            .child(toast.title.clone()),
    );

    header = header.child(title_content);

    {
        let dismiss_btn = div()
            .id(("toast-dismiss", toast_id))
            .size(px(18.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(4.0))
            .cursor_pointer()
            .hover(|el| el.bg(crate::theme::wash(0.08)))
            .on_click(cx.listener(move |this, _, window, cx| {
                on_dismiss(this, toast_id, window, cx);
            }))
            .child(
                crate::icons::icon(icons::CLOSE)
                    .size(px(11.0))
                    .text_color(theme.text_muted),
            );

        header = header.child(dismiss_btn);
    }

    card = card.child(header);

    // Optional description / subtitle
    if let Some(desc) = &toast.description {
        card = card.child(
            div()
                .text_size(px(11.5))
                .text_color(theme.text_muted)
                .child(desc.clone()),
        );
    }

    crate::frost::frosted(10.0, 16.0, card).into_any_element()
}

/// Render the bottom-right toast container holding all active toasts.
pub fn render_toast_overlay<S: 'static>(
    toasts: &[Toast],
    theme: &Theme,
    on_dismiss: impl Fn(&mut S, u64, &mut gpui::Window, &mut Context<S>) + 'static + Copy,
    cx: &mut Context<S>,
) -> Option<AnyElement> {
    if toasts.is_empty() {
        return None;
    }

    let mut container = div()
        .id("toast-overlay-container")
        .absolute()
        .bottom(px(18.0))
        .right(px(18.0))
        .flex()
        .flex_col_reverse()
        .gap(px(10.0))
        .occlude();

    for toast in toasts {
        container = container.child(render_toast_card(toast, theme, on_dismiss, cx));
    }

    Some(container.into_any_element())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toast_expiration() {
        let toast = Toast::new("Test");
        assert!(!toast.is_expired(Instant::now()));
        assert!(toast.is_expired(Instant::now() + Duration::from_secs(6)));
    }

    #[test]
    fn toast_card_matches_the_context_tooltip_glass() {
        for theme in [Theme::dark(), Theme::light()] {
            assert_eq!(toast_card_background(&theme), theme.composer_glass_bg());
        }
    }
}
