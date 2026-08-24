# Orchestrator User Message Bubble Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox syntax for tracking.

**Goal:** Port the verified visual and interaction contract of Orchestrator.dev's AgentUserMessageBubble into Comet's GPUI transcript without replacing Comet's native data, attachment, selection, or virtualization systems.

**Architecture:** Keep RowKind::User and the existing user_bubble_text/attachment pipeline. Add small pure helpers for summary and overflow decisions, measure shaped text through GPUI Div::on_children_prepainted, store overflow/preview state in Transcript, and render a native deferred full-message overlay parallel to the attachment lightbox.

**Tech Stack:** Rust 2024, GPUI, zeron-doc transcript rows, existing attachment cache/lightbox, existing selectable StyledText pipeline.

## Global Constraints

- Base the port on the complete reference component, its callers, and live Orchestrator inspection, not on screenshot inference.
- Preserve runtime adapters, document format, sync, attachment cache, image lightbox, text selection, virtualization, own-turn anchoring, assistant messages, tools, composer, and rail.
- User messages fill the existing 736 px transcript column and align to the leading edge.
- Use the verified reference values: 100 px maximum collapsed height, 12 px radius, 12 px horizontal padding, 8 px vertical padding, one-pixel input border, and 14 px text.
- Images and badge/mention rows align to the leading edge above the card.
- Overflow must use GPUI's measured child bounds, never a character-count heuristic.
- Overflowing cards paint a bottom fade and open a separate Full message overlay.
- Do not add a transcript-search feature; preserve selection/copy and file mentions.
- Use Context7-confirmed Div::on_children_prepainted and the repository's deferred/anchored overlay pattern.
- Follow TDD, run focused tests during implementation, and run the full workspace gate once at the end.
- Keep commits local; do not push, release, or promote to main.

---

## File Structure

- Modify crates/ui/src/transcript.rs: constants, pure helpers, overflow tracking, user-card rendering, preview state, and overlay.
- Reuse crates/ui/src/attachments.rs: image cache and existing image lightbox without changes.
- Reuse crates/ui/src/theme.rs: content background, border, foreground, muted text, and scrim tokens without adding a new palette.
- Test in crates/ui/src/transcript.rs: helper semantics, row projection preservation, and preview model behavior.

### Task 1: Pure user-message presentation helpers

**Files:**
- Modify: crates/ui/src/transcript.rs
- Test: crates/ui/src/transcript.rs

**Interfaces:**
- Produces: USER_MESSAGE_CARD_MAX_HEIGHT, USER_MESSAGE_CARD_PAD_Y, user_message_overflows(content_height), and user_message_attachment_summary(count).
- Consumed by: the card renderer and preview lifecycle in Task 2.

- [ ] **Step 1: Write failing tests for the missing helpers**

Append these tests to the existing transcript test module:

~~~rust
#[test]
fn user_message_overflow_uses_the_measured_content_height() {
    let content_limit = USER_MESSAGE_CARD_MAX_HEIGHT - USER_MESSAGE_CARD_PAD_Y * 2.0;
    assert!(!user_message_overflows(content_limit));
    assert!(user_message_overflows(content_limit + 0.5));
}

#[test]
fn image_only_user_messages_receive_the_reference_summary() {
    assert_eq!(user_message_attachment_summary(0), None);
    assert_eq!(user_message_attachment_summary(1).as_deref(), Some("Using image"));
    assert_eq!(
        user_message_attachment_summary(3).as_deref(),
        Some("Using 3 images"),
    );
}
~~~

- [ ] **Step 2: Run the focused tests and verify RED**

Run:

~~~bash
cargo test -p zeron-ui user_message_ --no-default-features
~~~

Expected: compilation fails because the constants and helper functions do not exist.

- [ ] **Step 3: Implement the minimal pure helpers**

Add beside the existing attachment/bubble constants:

~~~rust
pub const USER_MESSAGE_CARD_MAX_HEIGHT: f32 = 100.0;
pub const USER_MESSAGE_CARD_PAD_Y: f32 = 8.0;
pub const USER_MESSAGE_CARD_RADIUS: f32 = 12.0;
pub const USER_MESSAGE_FADE_HEIGHT: f32 = 40.0;

fn user_message_overflows(content_height: f32) -> bool {
    content_height > USER_MESSAGE_CARD_MAX_HEIGHT - USER_MESSAGE_CARD_PAD_Y * 2.0
}

fn user_message_attachment_summary(count: usize) -> Option<SharedString> {
    match count {
        0 => None,
        1 => Some("Using image".into()),
        count => Some(format!("Using {count} images").into()),
    }
}
~~~

- [ ] **Step 4: Run the focused tests and verify GREEN**

Run:

~~~bash
cargo test -p zeron-ui user_message_ --no-default-features
~~~

Expected: both helper tests pass.

- [ ] **Step 5: Format and commit**

~~~bash
cargo fmt --all -- --check
git add crates/ui/src/transcript.rs
git commit -m "test: define user message presentation contract"
~~~

### Task 2: Full-width measured user-message card

**Files:**
- Modify: crates/ui/src/transcript.rs
- Test: crates/ui/src/transcript.rs

**Interfaces:**
- Consumes: USER_MESSAGE_CARD_* and helper functions from Task 1.
- Produces: leading-aligned attachments/badges, full-width reference card, measured overflow map, and UserMessagePreview state.

- [ ] **Step 1: Add failing state-contract tests**

Add a pure preview constructor and test it before implementation:

~~~rust
#[test]
fn user_message_preview_preserves_text_and_mentions() {
    let mentions = Arc::new(vec![crate::composer::SentMentionSpan {
        range: 0..7,
        path: "src/lib.rs".into(),
        is_dir: false,
    }]);
    let preview = user_message_preview(
        "row-1".into(),
        "src/lib".into(),
        mentions.clone(),
    );
    assert_eq!(preview.row_id, "row-1");
    assert_eq!(preview.text, "src/lib");
    assert_eq!(preview.mentions, mentions);
}
~~~

- [ ] **Step 2: Run the preview test and verify RED**

Run:

~~~bash
cargo test -p zeron-ui user_message_preview_preserves_text_and_mentions --no-default-features
~~~

Expected: compilation fails because UserMessagePreview and user_message_preview are missing.

- [ ] **Step 3: Add overflow and preview state to Transcript**

Add:

~~~rust
#[derive(Clone)]
struct UserMessagePreview {
    row_id: SharedString,
    text: SharedString,
    mentions: Arc<Vec<crate::composer::SentMentionSpan>>,
}

fn user_message_preview(
    row_id: SharedString,
    text: SharedString,
    mentions: Arc<Vec<crate::composer::SentMentionSpan>>,
) -> UserMessagePreview {
    UserMessagePreview {
        row_id,
        text,
        mentions,
    }
}
~~~

Add fields to Transcript:

~~~rust
user_message_overflow: HashMap<SharedString, bool>,
user_message_preview: Option<UserMessagePreview>,
user_message_preview_focus: gpui::FocusHandle,
~~~

Initialize them in Transcript::new:

~~~rust
user_message_overflow: HashMap::new(),
user_message_preview: None,
user_message_preview_focus: cx.focus_handle(),
~~~

- [ ] **Step 4: Align attachments and badge rows to the leading edge**

In render_user_attachments replace justify_end with justify_start and retain all current dimensions, progress overlays, retries, and lightbox behavior.

In the RowKind::User badge row replace justify_end with justify_start. Do not modify badge rendering.

- [ ] **Step 5: Replace the current trailing bubble with the measured reference card**

For non-empty text, build one full-width card. The first direct child must be the full selectable text element so the prepaint listener can measure child bounds:

~~~rust
let overflow = self
    .user_message_overflow
    .get(&row.id)
    .copied()
    .unwrap_or(false);
let overflow_key = row.id.clone();
let weak = cx.weak_entity();
let preview = user_message_preview(row.id.clone(), text.clone(), mentions.clone());

let mut card = div()
    .id(SharedString::from(format!("{}#user-card", row.id)))
    .relative()
    .w_full()
    .max_h(px(USER_MESSAGE_CARD_MAX_HEIGHT))
    .overflow_hidden()
    .rounded(px(USER_MESSAGE_CARD_RADIUS))
    .border_1()
    .border_color(theme.border_strong)
    .bg(theme.bg)
    .shadow_sm()
    .px(px(12.0))
    .py(px(USER_MESSAGE_CARD_PAD_Y))
    .text_size(px(14.0))
    .line_height(px(22.0))
    .text_color(theme.text)
    .when(pending, |el| el.opacity(0.65))
    .child(user_bubble_text(&row.id, text, mentions, &theme))
    .on_children_prepainted(move |bounds, _, cx| {
        let measured = bounds
            .first()
            .map(|bounds| f32::from(bounds.size.height))
            .unwrap_or(0.0);
        let next = user_message_overflows(measured);
        weak.update(cx, |this, cx| {
            if this.user_message_overflow.insert(overflow_key.clone(), next) != Some(next) {
                cx.notify();
            }
        })
        .ok();
    });
~~~

When overflow is true, add pointer/hover behavior and open the preview without changing the document:

~~~rust
let weak = cx.weak_entity();
card = card
    .cursor_pointer()
    .hover(|style| style.border_color(theme.accent.opacity(0.40)))
    .on_click(move |_, window, cx| {
        weak.update(cx, |this, cx| {
            this.user_message_preview = Some(preview.clone());
            window.focus(&this.user_message_preview_focus, cx);
            cx.notify();
        })
        .ok();
    })
    .child(
        div()
            .absolute()
            .bottom_0()
            .left_0()
            .right_0()
            .h(px(USER_MESSAGE_FADE_HEIGHT))
            .bg(gpui::linear_gradient(
                0.0,
                gpui::linear_color_stop(theme.bg, 0.0),
                gpui::linear_color_stop(theme.bg.opacity(0.0), 1.0),
            )),
    );
~~~

Insert card directly into the user column. Remove justify_end, the 80% width cap, user_bubble_bg, and the 16×10 padding.

- [ ] **Step 6: Render an image-only summary card**

When text is empty and attachments are present, append the muted italic full-width reference card:

~~~rust
if text.is_empty()
    && let Some(summary) = user_message_attachment_summary(attachments.len())
{
    column = column.child(
        div()
            .w_full()
            .rounded(px(USER_MESSAGE_CARD_RADIUS))
            .border_1()
            .border_color(theme.border_strong)
            .bg(theme.bg)
            .shadow_sm()
            .px(px(12.0))
            .py(px(USER_MESSAGE_CARD_PAD_Y))
            .text_size(px(14.0))
            .text_color(theme.text_muted)
            .italic()
            .when(pending, |el| el.opacity(0.65))
            .child(summary),
    );
}
~~~

- [ ] **Step 7: Verify focused transcript tests and commit**

Run:

~~~bash
cargo fmt --all
cargo test -p zeron-ui user_message_ --no-default-features
cargo test -p zeron-ui transcript::tests --no-default-features
git diff --check
~~~

Expected: helper/preview and all transcript tests pass.

~~~bash
git add crates/ui/src/transcript.rs
git commit -m "feat: port Orchestrator user message card"
~~~

### Task 3: Full-message overlay

**Files:**
- Modify: crates/ui/src/transcript.rs
- Test: crates/ui/src/transcript.rs

**Interfaces:**
- Consumes: UserMessagePreview and preview focus/state from Task 2.
- Produces: deferred Full message overlay with selectable text, file mentions, scroll, Escape close, and scrim close.

- [ ] **Step 1: Add a pure overlay-size test**

Add:

~~~rust
#[test]
fn full_message_dialog_respects_the_reference_viewport_cap() {
    let viewport = gpui::size(px(1200.0), px(800.0));
    let (max_width, max_height) = full_message_dialog_limits(viewport);
    assert_eq!(max_width, px(672.0));
    assert_eq!(max_height, px(640.0));
}
~~~

- [ ] **Step 2: Run the overlay test and verify RED**

Run:

~~~bash
cargo test -p zeron-ui full_message_dialog_respects_the_reference_viewport_cap --no-default-features
~~~

Expected: compilation fails because full_message_dialog_limits is missing.

- [ ] **Step 3: Implement limits and overlay**

Add:

~~~rust
fn full_message_dialog_limits(
    viewport: gpui::Size<Pixels>,
) -> (Pixels, Pixels) {
    (
        px(f32::from(viewport.width).min(672.0)),
        px(f32::from(viewport.height) * 0.80),
    )
}
~~~

Add this overlay beside the attachment lightbox integration:

~~~rust
fn user_message_dialog(
    viewport: gpui::Size<Pixels>,
    preview: &UserMessagePreview,
    focus: &gpui::FocusHandle,
    theme: &Theme,
    on_close: impl Fn(&mut Window, &mut gpui::App) + 'static,
) -> AnyElement {
    let (max_w, max_h) = full_message_dialog_limits(viewport);
    let on_close = std::rc::Rc::new(on_close);
    let close_on_key = on_close.clone();
    let close_on_scrim = on_close.clone();
    let message = user_bubble_text(
        &SharedString::from(format!("{}#full", preview.row_id)),
        preview.text.clone(),
        preview.mentions.clone(),
        theme,
    );

    gpui::deferred(
        gpui::anchored()
            .position(gpui::point(px(0.0), px(0.0)))
            .child(
                div()
                    .id("full-user-message-scrim")
                    .occlude()
                    .track_focus(focus)
                    .w(viewport.width)
                    .h(viewport.height)
                    .bg(crate::popover::scrim_alpha(0.70))
                    .flex()
                    .items_center()
                    .justify_center()
                    .on_key_down(move |event: &gpui::KeyDownEvent, window, cx| {
                        if event.keystroke.key == "escape" {
                            cx.stop_propagation();
                            close_on_key(window, cx);
                        }
                    })
                    .on_click(move |_, window, cx| close_on_scrim(window, cx))
                    .child(
                        div()
                            .id("full-user-message-card")
                            .w(max_w)
                            .max_h(max_h)
                            .min_h_0()
                            .flex()
                            .flex_col()
                            .overflow_hidden()
                            .rounded(px(USER_MESSAGE_CARD_RADIUS))
                            .border_1()
                            .border_color(theme.border)
                            .bg(theme.surface_dialog)
                            .shadow_lg()
                            .on_click(|_, _, cx| cx.stop_propagation())
                            .child(
                                div()
                                    .flex_none()
                                    .px(px(16.0))
                                    .pt(px(16.0))
                                    .pb(px(10.0))
                                    .text_size(px(13.0))
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .text_color(theme.text_muted)
                                    .child("Full message"),
                            )
                            .child(
                                div()
                                    .min_h_0()
                                    .overflow_y_scroll()
                                    .px(px(16.0))
                                    .pb(px(16.0))
                                    .text_size(px(14.0))
                                    .line_height(px(22.0))
                                    .text_color(theme.text)
                                    .child(message),
                            ),
                    ),
            ),
    )
    .into_any_element()
}
~~~

- [ ] **Step 4: Paint the overlay from Transcript::render**

After building root and after the attachment-preview branch, add:

~~~rust
if let Some(preview) = self.user_message_preview.clone() {
    let weak = cx.weak_entity();
    return root.child(user_message_dialog(
        window.viewport_size(),
        &preview,
        &self.user_message_preview_focus,
        &theme,
        move |_, cx| {
            weak.update(cx, |this, cx| {
                this.user_message_preview = None;
                cx.notify();
            })
            .ok();
        },
    ));
}
~~~

Ensure opening either preview type clears the other preview state so only one modal can exist.

- [ ] **Step 5: Run tests, detector, and commit**

Run:

~~~bash
cargo fmt --all
cargo test -p zeron-ui full_message_dialog_respects_the_reference_viewport_cap --no-default-features
cargo test -p zeron-ui transcript::tests --no-default-features
node /Users/guilhermevarela/.agents/skills/impeccable/scripts/detect.mjs --json crates/ui/src/transcript.rs
git diff --check
~~~

Expected: tests pass and the one permitted detector run returns no actionable finding.

~~~bash
git add crates/ui/src/transcript.rs
git commit -m "feat: add full user message preview"
~~~

### Task 4: Integration and visual gate

**Files:**
- Verify: crates/ui/src/transcript.rs and the current feature branch

**Interfaces:**
- Produces: current test/build evidence and one bounded visual inspection of short, multiline, attachment, pending, image-only, and long user messages.

- [ ] **Step 1: Run source and focused gates**

~~~bash
cargo fmt --all -- --check
git diff --check
cargo test -p zeron-ui user_message_ --no-default-features
cargo test -p zeron-ui transcript::tests --no-default-features
cargo test -p zeron-ui composer::tests --no-default-features
cargo test -p zeron-ui attachments::tests --no-default-features
~~~

Expected: all commands pass.

- [ ] **Step 2: Run complete compilation once**

~~~bash
cargo check --workspace
cargo build -p zeron
~~~

Expected: both commands succeed with no new warning introduced by this port.

- [ ] **Step 3: Restart dev app and inspect the real surface**

Restart only the target/debug/zeron process launched from this worktree:

~~~bash
RUST_LOG=warn cargo run -p zeron
~~~

In one visual pass confirm:

- short and multiline messages fill the transcript column and align left;
- attachments and badges align left above the card;
- inline file mentions remain styled and selectable;
- pending opacity remains visible;
- long messages clamp at 100 px with a clean bottom fade;
- clicking a clipped card opens the complete selectable message;
- Escape and scrim close the overlay;
- image lightbox still works;
- no card, attachment, or following assistant row clips or shifts incorrectly.

If one grouped correction is required, fix it once, rerun the focused tests, rebuild, and perform one final confirmation only.

- [ ] **Step 4: Review and record state**

Perform an evidence-first review over da64f35..HEAD. Fix every Critical/Important finding with a focused regression test, then run:

~~~bash
git status --short --branch
git log -6 --oneline
pgrep -fl 'target/debug/zeron' || true
~~~

Expected: clean local feature branch and the updated dev app active. Do not push or promote.
