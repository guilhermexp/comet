//! Session navigation — the horizontal tab strip is gone (wing 2026-08-10):
//! the activity sidebar IS the session list, and the titlebar names the
//! selected session (harness brand icon + title). A `+` new-session button
//! lives in the titlebar's left control cluster while an existing session is
//! selected. `UiSettings.open_tabs` is legacy — no longer read or written.

use super::*;

/// The chat one step from `selected` in the sidebar `order`, wrapping at both
/// ends. Pure.
///
/// With nothing selected — the new-session canvas — cycling enters the list at
/// the end it would have wrapped to: the first row going forward, the last
/// going back. A selection that has since left the list (archived from another
/// device mid-cycle) is treated the same way rather than dead-ending.
pub(super) fn cycle_target(
    order: &[String],
    selected: Option<&str>,
    forward: bool,
) -> Option<String> {
    if order.is_empty() {
        return None;
    }
    let at = selected.and_then(|id| order.iter().position(|c| c == id));
    let next = match (at, forward) {
        (Some(at), true) => (at + 1) % order.len(),
        (Some(at), false) => (at + order.len() - 1) % order.len(),
        (None, true) => 0,
        (None, false) => order.len() - 1,
    };
    Some(order[next].clone())
}

pub(super) fn right_pane_expand_icon(expanded: bool) -> &'static str {
    if expanded {
        icons::COLLAPSE_ARROWS
    } else {
        icons::EXPAND_ARROWS
    }
}

/// The session header's "+" and fork buttons (28px each, 2px gap) plus the
/// row gap they cost the project-actions control.
const SESSION_CONTROLS_WIDTH: f32 = 28.0 * 2.0 + 2.0 + 8.0;

impl Shell {
    /// The same binding navigates the focused pane. Keep the persisted action
    /// IDs so existing user keymaps and the native browser bridge still work.
    pub(super) fn cycle_navigation(
        &mut self,
        forward: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.navigation_overlay_open(cx) {
            return;
        }
        if matches!(self.route, Route::Chat)
            && self.right_pane_open(cx)
            && self.navigation_focus.in_right(window, cx)
        {
            let rows = self.right_surface_rows(cx);
            if rows.len() <= 1 {
                return;
            }
            let active = self.resolved_right_active(cx);
            let at = rows.iter().position(|(surface, ..)| *surface == active);
            let next = match (at, forward) {
                (Some(at), true) => (at + 1) % rows.len(),
                (Some(at), false) => (at + rows.len() - 1) % rows.len(),
                (None, true) => 0,
                (None, false) => rows.len() - 1,
            };
            self.activate_right_surface(rows[next].0, window, cx);
        } else {
            self.cycle_session(forward, cx);
        }
    }

    fn navigation_overlay_open(&self, cx: &App) -> bool {
        self.overlay_owns_keyboard(cx)
            || self.sync_flow.has_visible_overlay()
            || self.delete_confirm.is_some()
            || self.delete_space_confirm.is_some()
            || self.rename_dialog.is_some()
            || self.rename_space_dialog.is_some()
            || self.chat_menu.get().is_some()
            || self.space_menu.get().is_some()
            || self.user_menu.get().is_some()
            || self.spaces_menu.get().is_some()
            || self.right_plus.get().is_some()
            || !self.pending_file_closes.is_empty()
            || self.pending_exit.is_some()
    }

    pub(super) fn activate_right_surface(
        &mut self,
        surface: RightSurface,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.navigation_focus.right_was_focused = true;
        self.composer
            .update(cx, |composer, _| composer.focus_pending = false);
        // Establish a stable target before detaching the old editor. Read-only
        // surfaces keep it; editable surfaces claim their own focus below/on mount.
        window.focus(&self.navigation_focus.right, cx);
        self.set_right_active(surface, cx);
        self.focus_right_file_editor(surface, window, cx);
    }

    pub(super) fn restore_right_focus_after_close(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.right_pane_open(cx) {
            self.activate_right_surface(self.resolved_right_active(cx), window, cx);
        } else {
            self.navigation_focus.right_was_focused = false;
            window.focus(&self.composer.focus_handle(cx), cx);
        }
    }

    /// Navigation requests focus once the destination composer renders.
    pub(super) fn focus_composer(&mut self, cx: &mut Context<Self>) {
        self.composer.update(cx, |composer, cx| {
            composer.focus_pending = true;
            cx.notify();
        });
    }

    /// Ctrl+Tab / Ctrl+Shift+Tab: step through the sidebar's Sessions list in
    /// the order it is drawn. Selection is immediate (no MRU overlay held open
    /// on the modifier) — one press, one session.
    ///
    /// Works from Settings too, landing back in chat like a jump; the
    /// shortcut recorder intercepts these keys while it records. Quiet under
    /// a keyboard-owning overlay: gpui dispatches a matched binding before any
    /// `on_key_down`, so a cycle under the add-space palette would strand the
    /// overlay over a session the user never picked.
    pub(super) fn cycle_session(&mut self, forward: bool, cx: &mut Context<Self>) {
        if self.overlay_owns_keyboard(cx) {
            return;
        }
        // The same list `render_active_rows` draws and jump shortcuts count.
        let order = self.sidebar_visible_order(cx);
        let selected = self.state.read(cx).selected_chat.clone();
        if let Some(target) = cycle_target(&order, selected.as_deref(), forward) {
            self.open_chat(target, cx);
        }
    }

    /// Boot landing: the most recently active visible chat once the first
    /// chats frame has synced (manual selection wins; no chats → the
    /// new-session canvas shows).
    pub(super) fn boot_select_chat(&mut self, cx: &mut Context<Self>) {
        let first = {
            let state = self.state.read(cx);
            if !state.chats_synced || state.selected_chat.is_some() || state.auto_selected {
                return;
            }
            state
                .overview_chats(Utc::now())
                .first()
                .map(|(_, c)| c.id.clone())
        };
        if let Some(first) = first {
            self.focus_composer(cx);
            self.state
                .update(cx, |s, cx| s.select_chat(Some(first), cx));
        }
    }

    /// Open a session from the sidebar: select it, the main area follows.
    pub(crate) fn open_chat(&mut self, chat_id: String, cx: &mut Context<Self>) {
        self.command_palette = None;
        self.route = Route::Chat;
        self.focus_composer(cx);
        self.state
            .update(cx, |s, cx| s.select_chat(Some(chat_id), cx));
        cx.notify();
    }

    /// `+` in the titlebar: open the new-session canvas. A set sidebar filter
    /// re-homes the canvas onto that project; under "All" the current pick
    /// (the last selected project, restored from composer defaults) stands.
    ///
    /// A new chat always starts with the terminal hidden: when the drawer is
    /// open it just hides (detach, not close — the source chat's tabs and
    /// PTYs survive for the return trip).
    pub(super) fn open_new_session(&mut self, cx: &mut Context<Self>) {
        self.command_palette = None;
        if self.sidebar_mode == SidebarMode::Workers {
            self.workers_model.update(cx, |model, cx| {
                let Some(project) = model.selected_project().cloned() else {
                    return;
                };
                let project_id = project.id.clone();
                let request = model
                    .presets()
                    .iter()
                    .find(|preset| preset.enabled && preset.quick_launch)
                    .or_else(|| model.presets().iter().find(|preset| preset.enabled))
                    .map(|preset| {
                        WorkersLaunchRequest::preset(project_id.clone(), preset.id.clone())
                    })
                    .unwrap_or_else(|| WorkersLaunchRequest::terminal(project_id))
                    .with_optional_worktree(
                        project
                            .worktree_branch
                            .as_ref()
                            .map(|_| project.path.clone()),
                        project.worktree_branch.clone(),
                    );
                model.launch(request, cx);
            });
            return;
        }
        self.route = Route::Chat;
        self.focus_composer(cx);
        let target = {
            let state = self.state.read(cx);
            self.settings
                .space_filter
                .clone()
                .filter(|id| state.space_row(id).is_some())
        };
        let defaults = crate::settings::composer::ComposerDefaults::load(&self.data_dir);
        self.state.update(cx, |s, cx| {
            if target.is_some() {
                s.select_space(target, cx);
            } else if defaults.no_project {
                // Opening an existing project session (including boot's last
                // session) must not erase the saved new-session opt-out.
                s.select_space(None, cx);
                if let Some(device) = defaults.device {
                    s.select_device(device, cx);
                }
            }
            s.select_chat(None, cx);
        });
        cx.notify();
    }

    /// The unified titlebar in chat mode:
    /// `[new-session +] [harness icon + session title] … [utility controls]`.
    /// Replaces the tab strip; inherits its titlebar duties (drag region,
    /// animated left inset, terminal and utility-panel controls, and the
    /// project actions control).
    pub(super) fn render_session_title_bar(
        &mut self,
        viewport_height: Pixels,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = Theme::of(cx).clone();
        // The canvas titles as NOTHING (user request — a "New session"
        // header over the empty canvas was noise); the bar keeps its height,
        // drag region, and buttons. A session appends its target as a muted
        // "project @ device" tag right of the title (the composer footer no
        // longer carries it).
        let (title, target, harness, on_canvas): (
            SharedString,
            Option<SharedString>,
            Option<zeron_proto::HarnessId>,
            bool,
        ) = {
            let state = self.state.read(cx);
            match state.selected_chat_row() {
                Some(chat) => {
                    let folder = chat
                        .space_id
                        .as_deref()
                        .and_then(|id| state.space_row(id))
                        .map(|s| s.display_name().to_string())
                        .unwrap_or_else(|| "~".to_string());
                    let device = state
                        .device_name(&chat.device_id)
                        .unwrap_or("Unknown device");
                    (
                        SharedString::from(transcript::single_line(
                            &chat.title.clone().unwrap_or_else(|| "New session".into()),
                        )),
                        Some(SharedString::from(format!("{folder} @ {device}"))),
                        chat.config.as_ref().map(|c| c.harness),
                        false,
                    )
                }
                None => (SharedString::from(""), None, None, true),
            }
        };
        let has_space = !self.state.read(cx).spaces.is_empty();
        // One registry: the pane is either open (hosting some surface) or not.
        let pane_open = self.right_pane_open(cx);
        let terminal_active =
            pane_open && matches!(self.resolved_right_active(cx), RightSurface::Terminal(_));

        // The new-session `+` renders in the WINDOW-CONTROL CLUSTER whenever a
        // session is selected (`render_titlebar_cluster`) — this row budgets
        // one button slot so the title never sits under it.
        let sidebar_now = self.sidebar_now();
        let plus_inset = TITLEBAR_ACTION_SLOT_WIDTH * self.titlebar_plus_alpha(cx);
        let details_open = self.details_sidebar_open(cx);
        let details_now = if details_open {
            self.eval_tween(self.details_tween, self.details_target(cx))
        } else {
            0.0
        };
        let right_open = self.right_pane_open(cx);
        let right_now = if right_open {
            self.eval_tween(self.right_tween, self.right_target(cx))
        } else {
            0.0
        };
        // Same glide as the old strip: content starts at the inset card's
        // left edge while the sidebar is open, and slides toward the control
        // cluster as it collapses.
        let content_left =
            (sidebar_now + Theme::SPACE_LG).max(self.title_bar_content_start() + plus_inset);
        let launcher_menu = (has_space && !pane_open && self.utility_add_menu_open)
            .then(|| self.render_utility_menu(true, cx));

        // Trailing titlebar section. With the changes pane open this is the
        // PANE'S HEADER — a strip exactly as wide as the pane carrying its
        // controls (scope dropdown, ref selector, fold-all from the Changes
        // entity; expand + close shell-side). It lives up here because the
        // titlebar overlay owns this band's hit-testing: controls mounted in
        // the pane itself would sit under the drag region and never see a
        // click. Hidden on the new-session canvas — nothing to diff yet.
        let changes_active = pane_open;
        let takeover = changes_active && self.right_pane_expanded;
        // In takeover the title hides and the strip owns the whole band, so
        // the row's left inset pulls back to the sidebar seam — the title
        // inset would push the scope dropdown off the pane's own left gutter
        // (user report: misaligned dead space). With the sidebar COLLAPSED
        // the seam is the window edge, where the traffic lights + nav
        // cluster overlay lives — the strip must still clear it, but only
        // just: `title_bar_content_start` carries the identity-group margin the
        // strip doesn't want (it brings its own 8px pad), and doubling up
        // read as a hole after the `+` (user report).
        let row_left = if takeover {
            // The surface tabs must LEFT-ALIGN with the pane's own rows (the
            // diff options and stats strip carry an 8px box gutter off the
            // seam — user report: rows started at different insets). The
            // strip's width is capped to `avail`, which subtracts the row's
            // 8px child gap — pulling row_left 8 LEFT of the seam cancels
            // that, so the uncapped strip starts exactly at the seam and its
            // own 8px pad lands the first chip on the pane gutter. The
            // window-control cluster still wins while the sidebar is
            // collapsed (the chips clear it instead of underlapping).
            let cluster_end =
                self.title_bar_content_start() - TITLEBAR_IDENTITY_GAP + plus_inset - 14.0;
            (sidebar_now - 8.0).max(cluster_end)
        } else {
            content_left
        };
        // Trailing cluster. It used to be sized like the PANE'S HEADER
        // (`width = right_now - pr`) because the surface chips lived up here;
        // they moved into the pane, and that width now only reached backwards
        // across the chat column - the capture button landed on top of the
        // conversation title. A content-sized cluster needs no geometry: the
        // row's own right padding already ends at the chat column's edge.
        let row_gap = 8.0;
        let files_open = self.files_panel_open(cx);
        let files_now = self.files_visible_width(cx);
        let changes_trailing: Option<gpui::AnyElement> = if changes_active && !on_canvas {
            let capabilities = titlebar_capabilities(
                SidebarMode::Orchestrator,
                !self.active_chat.is_empty(),
                false,
            );
            Some(
                div()
                    .flex_none()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(6.0))
                    .when(capabilities.trajectory, |el| {
                        el.child(self.render_orchestrator_trajectory_button(&theme, cx))
                    })
                    .when(!files_open, |el| {
                        el.child(self.render_files_panel_toggle(&theme, cx))
                    })
                    .when(!details_open && self.details_context(cx).is_some(), |el| {
                        el.child(self.render_details_sidebar_button(
                            "orchestrator-toggle-details-sidebar-with-panel",
                            &theme,
                            cx,
                        ))
                    })
                    .into_any_element(),
            )
        } else {
            let capabilities = titlebar_capabilities(
                SidebarMode::Orchestrator,
                !self.active_chat.is_empty(),
                false,
            );
            Some(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .when(capabilities.trajectory, |el| {
                        el.child(self.render_orchestrator_trajectory_button(&theme, cx))
                    })
                    .when(!on_canvas && !files_open, |el| {
                        el.child(self.render_files_panel_toggle(&theme, cx))
                    })
                    .when(!details_open && self.details_context(cx).is_some(), |el| {
                        el.child(self.render_details_sidebar_button(
                            "orchestrator-toggle-details-sidebar",
                            &theme,
                            cx,
                        ))
                    })
                    .into_any_element(),
            )
        };
        // The pane's surface tabs live IN the titlebar band, exactly as the
        // Workers header does (`panel_header`, shell.rs): the titlebar overlay
        // owns this band's hit-testing, so a strip mounted inside the pane had
        // to start 38px lower - the empty stripe above the tabs (user report).
        let panel_header = (changes_active
            && !on_canvas
            && !matches!(self.resolved_right_active(cx), RightSurface::Picker))
        .then(|| {
            div()
                .absolute()
                .top_0()
                .right(px(details_now + files_now))
                .w(px(right_now))
                .h(px(Theme::TITLEBAR_HEIGHT))
                .flex()
                .items_center()
                .gap(px(4.0))
                .pr(px(10.0))
                .pt(px(Theme::TITLEBAR_TOP_PAD))
                .occlude()
                // No left pad: the strip brings its own 8px gutter, which is
                // what lands the first chip on the pane's own gutter.
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .child(self.render_right_tab_strip(cx)),
                )
                .child(header_icon_button(
                    "expand-changes",
                    right_pane_expand_icon(self.right_pane_expanded),
                    takeover,
                    &theme,
                    cx.listener(|this, _, _, cx| this.toggle_right_pane_expand(cx)),
                ))
                .child(header_icon_button(
                    "toggle-changes",
                    icons::SIDEBAR_MINIMALISTIC,
                    true,
                    &theme,
                    cx.listener(|this, _, window, cx| this.toggle_right_pane(window, cx)),
                ))
        });
        // The explorer slot (upstream's docked Files column) sits over the
        // explorer column between the surface host and the Details sidebar,
        // carrying its own toggle while the explorer is open; closed, the
        // toggle rides in the trailing cluster above.
        let files_header = (files_open && !on_canvas && files_now > 0.0).then(|| {
            div()
                .absolute()
                .top_0()
                .right(px(details_now))
                .w(px(files_now))
                .h(px(Theme::TITLEBAR_HEIGHT))
                .flex()
                .items_center()
                .justify_end()
                .pr(px(10.0))
                .pt(px(Theme::TITLEBAR_TOP_PAD))
                .overflow_hidden()
                .occlude()
                .child(self.render_files_panel_toggle(&theme, cx))
        });
        // Project actions (upstream #project-actions) take whatever the title
        // row leaves between the title and the trailing cluster.
        let trailing_budget = 4.0 * 28.0 + 3.0 * 6.0;
        let available_titlebar_width = (self.viewport_width
            - row_left
            - self.titlebar_right_pad(Theme::SPACE_LG)
            - details_now
            - right_now
            - files_now
            - trailing_budget
            - row_gap * 3.0)
            .max(0.0);

        // The session's own side-chat controls: "+" mints a fresh side chat
        // under this session, fork copies its history into one. Same pair
        // the side-chat header carries, so a family reads the same from
        // either end.
        let session_controls = (!takeover && !on_canvas).then(|| {
            let busy = self.side_chat_creating;
            div()
                .flex_none()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(2.0))
                .child(
                    header_icon_button(
                        "session-new-side-chat",
                        icons::PLUS,
                        false,
                        &theme,
                        cx.listener(|this, _, _, cx| this.create_child_chat(None, cx)),
                    )
                    .role(gpui::Role::Button)
                    .aria_label("New side chat")
                    .when(busy, |el| el.opacity(0.4)),
                )
                .child(
                    header_icon_button(
                        "session-fork",
                        icons::GIT_BRANCH,
                        false,
                        &theme,
                        cx.listener(|this, _, _, cx| this.create_side_chat(cx)),
                    )
                    .role(gpui::Role::Button)
                    .aria_label("Fork this session")
                    .when(busy, |el| el.opacity(0.4)),
                )
        });
        let available_titlebar_width = if session_controls.is_some() {
            (available_titlebar_width - SESSION_CONTROLS_WIDTH).max(0.0)
        } else {
            available_titlebar_width
        };
        let actions = (!takeover && !on_canvas)
            .then(|| {
                self.render_project_actions_control(available_titlebar_width, viewport_height, cx)
            })
            .flatten();
        let inner = div()
            .size_full()
            .flex()
            .items_center()
            .pt(px(Theme::TITLEBAR_TOP_PAD))
            .gap(px(row_gap))
            .pl(px(row_left))
            .pr(px(self.titlebar_right_pad(Theme::SPACE_LG)
                + details_now
                + files_now
                + right_now))
            // In panel takeover the header strip spans the whole band — the
            // title would sit UNDER it (both flex_none, the row overflows and
            // paint order stacks them), so it hides for the duration.
            .when(!takeover, |el| {
                el.child(
                    div()
                        .min_w_0()
                        .overflow_hidden()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(px(6.0))
                        .when_some(
                            harness.map(crate::pickers::harness_brand_icon),
                            |el, (path, tint)| {
                                el.child(
                                    icon(path)
                                        .size(px(14.0))
                                        .flex_none()
                                        .text_color(tint.unwrap_or(theme.text_muted)),
                                )
                            },
                        )
                        .child(
                            div()
                                .min_w_0()
                                .truncate()
                                .text_size(px(12.0))
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .text_color(if on_canvas {
                                    theme.text_muted.opacity(0.7)
                                } else {
                                    theme.text.opacity(0.85)
                                })
                                .child(title),
                        )
                        .when_some(target, |el, target| {
                            el.child(
                                div()
                                    .min_w_0()
                                    .truncate()
                                    .text_size(px(12.0))
                                    .text_color(theme.text_muted.opacity(0.5))
                                    .child(target),
                            )
                        }),
                )
            })
            .child(div().flex_1())
            .children(session_controls)
            .children(actions)
            .children(changes_trailing)
            // Stable utility controls at the right edge of the conversation
            // titlebar; hidden on the new-session canvas because neither
            // terminal nor changes has a session target there. Changes owns
            // the full trailing strip while active.
            .when(!changes_active && has_space && !on_canvas, |el| {
                el.child(header_icon_button(
                    "toggle-terminal",
                    icons::TERMINAL,
                    terminal_active,
                    &theme,
                    cx.listener(|this, _, window, cx| this.toggle_terminal(window, cx)),
                ))
            })
            .when(!changes_active && has_space && !on_canvas, |el| {
                el.child(
                    div()
                        .relative()
                        .size(px(28.0))
                        .child(header_icon_button(
                            "toggle-utility-panel",
                            icons::SIDEBAR_MINIMALISTIC,
                            pane_open,
                            &theme,
                            cx.listener(|this, _, window, cx| {
                                this.toggle_utility_panel(window, cx)
                            }),
                        ))
                        .when_some(launcher_menu, |button, menu| {
                            button.child(popover::anchored_menu(
                                "utility-launcher-menu-anchor",
                                menu,
                                None,
                            ))
                        }),
                )
            });

        // The unified window titlebar: full-width on the glass shell, ABOVE
        // the inset card. No bottom border — the card's own hairline is the
        // separation; the glass gutter shows between.
        let bar = div()
            .relative()
            .h(px(Theme::TITLEBAR_HEIGHT))
            .flex_none()
            .child(inner)
            .children(panel_header)
            .children(files_header);
        self.titlebar_drag_region("chat-titlebar", bar, cx)
            .into_any_element()
    }

    /// Toggle for upstream's docked explorer column (`files_panel.rs`). It
    /// rides the fork's trailing cluster while the explorer is closed and
    /// the explorer slot over its column while open.
    fn render_files_panel_toggle(&self, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let open = self.files_panel_open(cx);
        header_icon_button(
            "toggle-files-panel",
            icons::FILE_TREE,
            open,
            theme,
            cx.listener(|this, _, window, cx| this.toggle_files_panel(window, cx)),
        )
        .role(gpui::Role::Button)
        .aria_label(if open {
            "Hide files panel"
        } else {
            "Show files panel"
        })
        .into_any_element()
    }
}

#[cfg(test)]
mod cycle_tests {
    use super::*;

    fn order(ids: &[&str]) -> Vec<String> {
        ids.iter().map(|id| id.to_string()).collect()
    }

    #[test]
    fn steps_forward_and_back_through_the_list() {
        let list = order(&["a", "b", "c"]);
        assert_eq!(cycle_target(&list, Some("a"), true).as_deref(), Some("b"));
        assert_eq!(cycle_target(&list, Some("b"), true).as_deref(), Some("c"));
        assert_eq!(cycle_target(&list, Some("c"), false).as_deref(), Some("b"));
        assert_eq!(cycle_target(&list, Some("b"), false).as_deref(), Some("a"));
    }

    #[test]
    fn wraps_at_both_ends() {
        let list = order(&["a", "b", "c"]);
        assert_eq!(cycle_target(&list, Some("c"), true).as_deref(), Some("a"));
        assert_eq!(cycle_target(&list, Some("a"), false).as_deref(), Some("c"));
    }

    #[test]
    fn a_single_session_cycles_to_itself() {
        // Not a no-op by accident: with one row both directions must resolve,
        // so the shortcut never looks broken by dead-ending on `None`.
        let list = order(&["only"]);
        assert_eq!(
            cycle_target(&list, Some("only"), true).as_deref(),
            Some("only")
        );
        assert_eq!(
            cycle_target(&list, Some("only"), false).as_deref(),
            Some("only")
        );
    }

    #[test]
    fn no_selection_enters_the_list_from_the_matching_end() {
        let list = order(&["a", "b", "c"]);
        assert_eq!(cycle_target(&list, None, true).as_deref(), Some("a"));
        assert_eq!(cycle_target(&list, None, false).as_deref(), Some("c"));
        assert_eq!(
            cycle_target(&list, Some("gone"), true).as_deref(),
            Some("a")
        );
        assert_eq!(
            cycle_target(&list, Some("gone"), false).as_deref(),
            Some("c")
        );
    }

    #[test]
    fn an_empty_list_has_nothing_to_select() {
        assert_eq!(cycle_target(&[], None, true), None);
        assert_eq!(cycle_target(&[], Some("a"), true), None);
    }

    // Cycling walks the rows the sidebar is drawing, not every chat: that
    // guarantee is structural now — `cycle_session` reads the same
    // `AppState::sidebar_chats` the sidebar and the jump shortcuts read, and
    // `jump_slots_count_the_rows_the_sidebar_draws` (state.rs) covers the
    // space-filter behaviour for all of them.
}
