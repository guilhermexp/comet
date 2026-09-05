//! Floating toast notification system anchored in the bottom-right of the window.
//!
//! Provides transient, floating notification cards with customizable titles,
//! descriptions, action buttons, progress states, and auto-dismissal timers.

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
    ProviderUpdate {
        count: usize,
        clis: Vec<String>,
        updatable_count: usize,
    },
    UpdatingProgress {
        current: usize,
        total: usize,
        cli_name: String,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToastAction {
    ReviewWorkerUpdates,
    UpdateAllWorkerClis,
}

#[derive(Clone, Debug)]
pub struct Toast {
    pub id: u64,
    pub kind: ToastKind,
    pub title: SharedString,
    pub description: Option<SharedString>,
    pub primary_action: Option<(SharedString, ToastAction)>,
    pub secondary_action: Option<(SharedString, ToastAction)>,
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
            primary_action: None,
            secondary_action: None,
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

    pub fn provider_updates(count: usize, clis: Vec<String>, updatable_count: usize) -> Self {
        let title = if count == 1 {
            "1 provider update available".into()
        } else {
            format!("{count} provider updates available").into()
        };

        let description = if clis.is_empty() {
            None
        } else if clis.len() <= 2 {
            Some(clis.join(", ").into())
        } else {
            Some(format!("{}, {} +{} more", clis[0], clis[1], clis.len() - 2).into())
        };

        Self {
            id: next_toast_id(),
            kind: ToastKind::ProviderUpdate {
                count,
                clis,
                updatable_count,
            },
            title,
            description,
            primary_action: (updatable_count > 0)
                .then(|| ("Update all".into(), ToastAction::UpdateAllWorkerClis)),
            secondary_action: Some(("Review".into(), ToastAction::ReviewWorkerUpdates)),
            created_at: Instant::now(),
            duration: Some(Duration::from_secs(10)),
        }
    }

    pub fn updating_progress(current: usize, total: usize, cli_name: &str) -> Self {
        Self {
            id: next_toast_id(),
            kind: ToastKind::UpdatingProgress {
                current,
                total,
                cli_name: cli_name.to_owned(),
            },
            title: format!("Updating {current}/{total} ({cli_name})…").into(),
            description: None,
            primary_action: None,
            secondary_action: None,
            created_at: Instant::now(),
            duration: None, // persistent during update progress
        }
    }

    pub fn update_complete(succeeded: usize, failed: usize) -> Self {
        let title = if failed == 0 {
            if succeeded == 1 {
                "Updated 1 provider CLI".into()
            } else {
                format!("Updated {succeeded} provider CLIs").into()
            }
        } else {
            format!("Updated {succeeded} · {failed} failed").into()
        };

        let description = if failed > 0 {
            Some("Open Settings › Presets to update manually".into())
        } else {
            None
        };

        Self {
            id: next_toast_id(),
            kind: if failed == 0 {
                ToastKind::Success
            } else {
                ToastKind::Warning
            },
            title,
            description,
            primary_action: (failed > 0)
                .then(|| ("Review".into(), ToastAction::ReviewWorkerUpdates)),
            secondary_action: None,
            created_at: Instant::now(),
            duration: Some(Duration::from_secs(6)),
        }
    }
}

/// Render a single toast card matching Orchestrator.dev's styling.
pub fn render_toast_card<S: 'static>(
    toast: &Toast,
    theme: &Theme,
    on_action: impl Fn(&mut S, ToastAction, &mut gpui::Window, &mut Context<S>) + 'static + Copy,
    on_dismiss: impl Fn(&mut S, u64, &mut gpui::Window, &mut Context<S>) + 'static + Copy,
    cx: &mut Context<S>,
) -> AnyElement {
    let toast_id = toast.id;
    let is_progress = matches!(toast.kind, ToastKind::UpdatingProgress { .. });

    let mut card = div()
        .id(("toast-item", toast.id))
        .w(px(320.0))
        .p(px(14.0))
        .rounded(px(10.0))
        .border_1()
        .border_color(theme.border.opacity(0.85))
        .bg(theme.surface_overlay)
        .shadow_md()
        .flex()
        .flex_col()
        .gap(px(6.0));
    // Header row: Title + dismiss button
    let mut header = div().flex().items_start().justify_between().gap(px(8.0));

    let title_el = div()
        .text_size(px(13.0))
        .font_weight(gpui::FontWeight::MEDIUM)
        .text_color(theme.text)
        .child(toast.title.clone());

    header = header.child(title_el);

    if !is_progress {
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

    // Action buttons row (Review / Update all)
    if toast.primary_action.is_some() || toast.secondary_action.is_some() {
        let mut actions = div().mt(px(4.0)).flex().items_center().gap(px(8.0));

        if let Some((label, action)) = &toast.secondary_action {
            let action = *action;
            let sec_btn = div()
                .id(("toast-secondary-action", toast_id))
                .h(px(26.0))
                .px(px(10.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(6.0))
                .bg(crate::theme::wash(0.06))
                .cursor_pointer()
                .hover(|el| el.bg(crate::theme::wash(0.12)))
                .text_size(px(11.5))
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(theme.text)
                .on_click(cx.listener(move |this, _, window, cx| {
                    on_action(this, action, window, cx);
                }))
                .child(label.clone());
            actions = actions.child(sec_btn);
        }

        if let Some((label, action)) = &toast.primary_action {
            let action = *action;
            let prim_btn = div()
                .id(("toast-primary-action", toast_id))
                .h(px(26.0))
                .px(px(12.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(6.0))
                .bg(theme.solid)
                .cursor_pointer()
                .hover(|el| el.opacity(0.9))
                .text_size(px(11.5))
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(theme.on_solid)
                .on_click(cx.listener(move |this, _, window, cx| {
                    on_action(this, action, window, cx);
                }))
                .child(label.clone());
            actions = actions.child(prim_btn);
        }

        card = card.child(actions);
    }

    card.into_any_element()
}

/// Render the bottom-right toast container holding all active toasts.
pub fn render_toast_overlay<S: 'static>(
    toasts: &[Toast],
    theme: &Theme,
    on_action: impl Fn(&mut S, ToastAction, &mut gpui::Window, &mut Context<S>) + 'static + Copy,
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
        container = container.child(render_toast_card(toast, theme, on_action, on_dismiss, cx));
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
    fn provider_updates_formatting() {
        let single = Toast::provider_updates(1, vec!["Pi".into()], 1);
        assert_eq!(single.title.as_ref(), "1 provider update available");
        assert_eq!(single.description.as_deref(), Some("Pi"));
        assert!(single.primary_action.is_some());
        assert!(single.secondary_action.is_some());

        let multi = Toast::provider_updates(3, vec!["Pi".into(), "Codex".into(), "OMP".into()], 2);
        assert_eq!(multi.title.as_ref(), "3 provider updates available");
        assert_eq!(multi.description.as_deref(), Some("Pi, Codex +1 more"));
    }

    #[test]
    fn updating_progress_and_completion() {
        let progress = Toast::updating_progress(1, 2, "Pi");
        assert_eq!(progress.title.as_ref(), "Updating 1/2 (Pi)…");
        assert!(progress.duration.is_none());

        let complete_success = Toast::update_complete(2, 0);
        assert_eq!(complete_success.title.as_ref(), "Updated 2 provider CLIs");
        assert_eq!(complete_success.kind, ToastKind::Success);

        let complete_with_failures = Toast::update_complete(1, 1);
        assert_eq!(
            complete_with_failures.title.as_ref(),
            "Updated 1 · 1 failed"
        );
        assert_eq!(complete_with_failures.kind, ToastKind::Warning);
        assert!(complete_with_failures.primary_action.is_some());
    }
}
