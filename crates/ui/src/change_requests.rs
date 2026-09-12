//! In-memory checkout change-request state for desktop views.
//!
//! The workspace document remains the source of truth for chats and projects;
//! pull-request metadata is host-local, short-lived capability state and is
//! deliberately never written back into a synced document.

use std::cell::OnceCell;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use gpui::{AnyElement, Context, Render, SharedString, Window, div, prelude::*, px};
use zeron_proto::{ChangeRequestSummary, Chat, CheckoutChangeRequestStatus, Space};

use crate::theme::Theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ChangeRequestBadgeTone {
    Open,
    Merged,
    Closed,
}

impl ChangeRequestBadgeTone {
    pub fn color(self, theme: &Theme) -> gpui::Hsla {
        match self {
            Self::Open => theme.success,
            Self::Merged => theme.code_text,
            Self::Closed => theme.danger,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ChangeRequestBadgeModel {
    pub number: SharedString,
    pub state_label: &'static str,
    pub title: SharedString,
    pub tone: ChangeRequestBadgeTone,
}

impl ChangeRequestBadgeModel {
    pub fn from_summary(summary: &ChangeRequestSummary) -> Self {
        use zeron_proto::ChangeRequestState;

        let (state_label, tone) = match summary.state {
            ChangeRequestState::Open => ("Open", ChangeRequestBadgeTone::Open),
            ChangeRequestState::Merged => ("Merged", ChangeRequestBadgeTone::Merged),
            ChangeRequestState::Closed => ("Closed", ChangeRequestBadgeTone::Closed),
        };
        Self {
            number: format!("#{}", summary.number).into(),
            state_label,
            title: summary.title.replace(['\r', '\n'], " ").into(),
            tone,
        }
    }
}

pub(crate) struct ChangeRequestTooltip {
    model: ChangeRequestBadgeModel,
}

impl ChangeRequestTooltip {
    pub fn new(summary: &ChangeRequestSummary) -> Self {
        Self {
            model: ChangeRequestBadgeModel::from_summary(summary),
        }
    }
}

impl Render for ChangeRequestTooltip {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::of(cx);
        let card = div()
            .max_w(px(320.0))
            .px(px(9.0))
            .py(px(7.0))
            .flex()
            .flex_col()
            .gap(px(3.0))
            .rounded(px(6.0))
            .border_1()
            .border_color(theme.border_strong)
            .bg(if theme.is_frost() {
                theme.glass_overlay()
            } else {
                theme.surface_raised
            })
            .shadow_md()
            .child(
                div()
                    .text_size(px(11.0))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(self.model.tone.color(theme))
                    .child(SharedString::from(format!(
                        "PR {} · {}",
                        self.model.number, self.model.state_label
                    ))),
            )
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .whitespace_nowrap()
                    .text_size(px(11.0))
                    .text_color(theme.text_muted)
                    .child(self.model.title.clone()),
            );
        crate::frost::frosted(6.0, crate::frost::MENU_BLUR, card)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ChangeRequestBadgeSurface {
    Sidebar,
    SidebarIcon,
    Composer,
}

pub(crate) fn pull_request_badge(
    id: SharedString,
    summary: ChangeRequestSummary,
    surface: ChangeRequestBadgeSurface,
    theme: &Theme,
) -> AnyElement {
    let model = ChangeRequestBadgeModel::from_summary(&summary);
    let color = model.tone.color(theme);
    let url = summary.url.clone();
    let tooltip_summary = summary;
    let composer = surface == ChangeRequestBadgeSurface::Composer;
    let icon_only = surface == ChangeRequestBadgeSurface::SidebarIcon;

    div()
        .id(id)
        .h(px(if composer { 20.0 } else { 16.0 }))
        .flex_none()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(if composer { 5.0 } else { 0.0 }))
        .px(px(if composer { 7.0 } else { 4.0 }))
        .rounded(px(if composer { 6.0 } else { 4.0 }))
        .bg(color.opacity(0.08))
        .text_size(px(if composer { 11.0 } else { 10.0 }))
        .font_weight(gpui::FontWeight::MEDIUM)
        .text_color(color.opacity(0.85))
        .cursor_pointer()
        .hover(move |style| style.bg(color.opacity(0.16)).text_color(color))
        .on_click(move |_, _, cx| {
            cx.stop_propagation();
            cx.open_url(&url);
        })
        .tooltip(move |_, cx| {
            cx.new(|_| ChangeRequestTooltip::new(&tooltip_summary))
                .into()
        })
        .tooltip_show_delay(std::time::Duration::from_millis(350))
        .when(composer || icon_only, |element| {
            element.child(
                crate::icons::icon(crate::icons::PULL_REQUEST)
                    .size(px(11.0))
                    .flex_none()
                    .text_color(color.opacity(0.85)),
            )
        })
        // Monospace digits give the badge a stable tabular width as PR numbers change.
        .when(!icon_only, |element| {
            element.child(
                div()
                    .font_family(theme.font_mono.clone())
                    .child(model.number),
            )
        })
        .into_any_element()
}

/// One host-side checkout watch. Multiple chats can share this target.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct ChangeRequestWatchKey {
    pub device_id: String,
    pub cwd: String,
    pub branch: String,
    pub checkout_id: Option<String>,
}

#[derive(Debug, Default)]
pub(crate) struct ChangeRequestClientState {
    snapshots: HashMap<ChangeRequestWatchKey, CheckoutChangeRequestStatus>,
    /// The engine version that rejected this versioned capability. A device
    /// update publishes its running version, so a host upgrade invalidates this
    /// negative cache without polling older hosts.
    unsupported_devices: HashMap<String, Option<String>>,
}

impl ChangeRequestClientState {
    pub fn is_supported(&self, device_id: &str) -> bool {
        !self.unsupported_devices.contains_key(device_id)
    }

    pub fn mark_unsupported(&mut self, device_id: String, engine_version: Option<String>) {
        self.unsupported_devices
            .insert(device_id.clone(), engine_version);
        self.snapshots.retain(|key, _| key.device_id != device_id);
    }

    /// Forget a version-skew rejection after the host advertises a different
    /// engine version. `UnknownMethod` describes one running engine, not the
    /// device permanently.
    pub fn clear_unsupported_on_version_change(
        &mut self,
        device_id: &str,
        engine_version: Option<&str>,
    ) -> bool {
        let Some(unsupported_version) = self.unsupported_devices.get(device_id) else {
            return false;
        };
        if unsupported_version.as_deref() == engine_version {
            return false;
        }
        self.unsupported_devices.remove(device_id);
        true
    }

    pub fn store(&mut self, key: ChangeRequestWatchKey, snapshot: CheckoutChangeRequestStatus) {
        // A new frame replaces the old branch/checkout context for this path.
        // Rendering validates the complete identity again before exposing it.
        self.snapshots.insert(key, snapshot);
    }

    pub fn retain_targets(&mut self, targets: &HashSet<ChangeRequestWatchKey>) {
        self.snapshots.retain(|key, _| targets.contains(key));
    }

    pub fn change_request_for_chat<'a>(
        &'a self,
        chat: &Chat,
        spaces: &[Space],
    ) -> Option<&'a ChangeRequestSummary> {
        change_request_for_chat(chat, spaces, self.snapshots.values())
    }

    /// The PR resolved for a device-local checkout. A Workers project carries
    /// no chat identity to re-verify, so the checkout IS the identity — and
    /// both halves must match: a snapshot left over from the branch this
    /// worktree used to be on would otherwise name the wrong pull request.
    ///
    /// Known hole: `snapshots` is multi-device and this lookup has no device to
    /// filter by, so two devices sharing an absolute path *and* a branch name
    /// collide. Closing it means threading the local device id down from
    /// `AppState` (which owns `local_device_id`) into both callers.
    pub fn change_request_for_checkout(
        &self,
        cwd: &str,
        branch: &str,
    ) -> Option<&ChangeRequestSummary> {
        change_request_for_checkout(cwd, branch, self.snapshots.values())
    }
}

pub(crate) fn change_request_for_checkout<'a>(
    cwd: &str,
    branch: &str,
    snapshots: impl IntoIterator<Item = &'a CheckoutChangeRequestStatus>,
) -> Option<&'a ChangeRequestSummary> {
    let branch = branch.trim();
    if branch.is_empty() || cwd.is_empty() {
        return None;
    }
    let canonical_cwd = OnceCell::new();
    snapshots
        .into_iter()
        .find(|snapshot| {
            snapshot.branch == branch && same_checkout(&snapshot.cwd, cwd, &canonical_cwd)
        })
        .and_then(|snapshot| snapshot.change_request.as_ref())
}

/// Two checkout paths that name the same directory.
///
/// The host reports `git rev-parse --show-toplevel`, which resolves symlinks
/// (`/private/var/...` on macOS), while an adopted worktree keeps the path as
/// the user gave it (`/var/...`). Raw equality is tried first, so an exact
/// match costs no syscall, and a path that no longer exists simply fails to
/// match instead of erroring.
///
/// A raw miss is NOT free: it canonicalizes `cwd` once per call through the
/// shared cell (and short-circuits every later snapshot when that fails), then
/// `snapshot_cwd` for each snapshot that still reaches this point. Both are
/// blocking `realpath` calls, so every caller tests its cheap scalar fields —
/// device, branch, checkout — *before* this one. That ordering, not the cell,
/// is what keeps the syscall out of the render loop.
fn same_checkout(snapshot_cwd: &str, cwd: &str, canonical_cwd: &OnceCell<Option<PathBuf>>) -> bool {
    snapshot_cwd == cwd
        || canonical_cwd
            .get_or_init(|| canonical_path(cwd))
            .as_deref()
            .is_some_and(|canonical| canonical_path(snapshot_cwd).as_deref() == Some(canonical))
}

#[cfg(test)]
thread_local! {
    /// `canonical_path` calls on this thread, so a test can prove the render
    /// path made none.
    static CANONICALIZE_CALLS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

fn canonical_path(path: &str) -> Option<PathBuf> {
    #[cfg(test)]
    CANONICALIZE_CALLS.with(|calls| calls.set(calls.get() + 1));
    std::fs::canonicalize(path).ok()
}

/// Active, fully identified checkouts that need host-side PR resolution.
pub(crate) fn desired_watch_targets(
    chats: &[Chat],
    _spaces: &[Space],
    is_unsupported: impl Fn(&str) -> bool,
) -> HashSet<ChangeRequestWatchKey> {
    chats
        .iter()
        .filter(|chat| !chat.archived)
        .filter(|chat| !is_unsupported(&chat.device_id))
        .filter_map(|chat| {
            let source = chat.source_context.as_ref()?;
            let (cwd, branch, checkout_id) = (
                source.repo_root.as_str(),
                source.branch.as_str(),
                Some(source.checkout_id.clone()),
            );
            if branch.trim().is_empty() {
                return None;
            }
            Some(ChangeRequestWatchKey {
                device_id: chat.device_id.clone(),
                cwd: cwd.to_owned(),
                branch: branch.to_owned(),
                checkout_id,
            })
        })
        .collect()
}

/// Current branches of local checkouts and worktrees supplied by the Workers
/// working set. The same branch resolver drives their visible context labels.
pub(crate) fn workers_change_request_targets(
    projects: &[zeron_workers_unpeel::WorkersProject],
    local_device_id: &str,
) -> HashSet<ChangeRequestWatchKey> {
    projects
        .iter()
        .filter_map(|project| {
            let branch = project.change_request_branch()?.trim();
            if branch.is_empty() || project.path.trim().is_empty() {
                return None;
            }
            Some(ChangeRequestWatchKey {
                device_id: local_device_id.to_owned(),
                cwd: project.path.clone(),
                branch: branch.to_owned(),
                checkout_id: None,
            })
        })
        .collect()
}

pub(crate) fn watch_params(
    target: &ChangeRequestWatchKey,
    local_device_id: Option<&str>,
) -> serde_json::Value {
    let mut params = serde_json::json!({
        "cwd": target.cwd,
        "branch": target.branch,
    });
    if local_device_id != Some(target.device_id.as_str())
        && let Some(object) = params.as_object_mut()
    {
        object.insert(
            "targetDeviceId".into(),
            serde_json::Value::String(target.device_id.clone()),
        );
    }
    params
}

/// Resolve a snapshot for a row without trusting the cache key alone.
///
/// Only conversation-owned source context is trusted. Legacy scalar metadata
/// cannot prove that a worktree has not switched branches since it was written.
pub fn change_request_for_chat<'a>(
    chat: &Chat,
    spaces: &[Space],
    snapshots: impl IntoIterator<Item = &'a CheckoutChangeRequestStatus>,
) -> Option<&'a ChangeRequestSummary> {
    let branch = conversation_branch(chat, spaces)?.trim();
    if branch.is_empty() {
        return None;
    }
    let source = chat.source_context.as_ref()?;
    let cwd = source.repo_root.as_str();
    let checkout_id = chat
        .source_context
        .as_ref()
        .map(|source| source.checkout_id.as_str())
        .or(chat.checkout_id.as_deref());

    let canonical_cwd = OnceCell::new();
    snapshots
        .into_iter()
        .find(|snapshot| {
            // Every cheap scalar first: `same_checkout` can hit the filesystem,
            // and this runs per row per frame.
            snapshot.device_id == chat.device_id
                && snapshot.branch == branch
                && checkout_id.is_none_or(|checkout_id| {
                    !snapshot.checkout_id.is_empty() && snapshot.checkout_id == checkout_id
                })
                && same_checkout(&snapshot.cwd, cwd, &canonical_cwd)
        })
        .and_then(|snapshot| snapshot.change_request.as_ref())
}

/// Branch metadata safe to render for this conversation. Pre-source-context
/// rows are intentionally rejected: their scalar branch may describe an old
/// state of either a shared checkout or a worktree.
pub(crate) fn conversation_branch<'a>(chat: &'a Chat, _spaces: &'a [Space]) -> Option<&'a str> {
    chat.source_context
        .as_ref()
        .map(|source| source.branch.as_str())
        .filter(|branch| !branch.trim().is_empty())
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone as _, Utc};
    use zeron_proto::ChangeRequestState;

    use super::*;

    fn chat(id: &str, device: &str, cwd: Option<&str>, checkout: Option<&str>) -> Chat {
        Chat {
            id: id.into(),
            device_id: device.into(),
            title: None,
            archived: false,
            cwd: cwd.map(str::to_owned),
            branch: Some("feature/pr".into()),
            checkout_id: checkout.map(str::to_owned),
            source_context: None,
            config: None,
            last_message_preview: None,
            last_message_at: None,
            created_at: Utc.timestamp_opt(0, 0).unwrap(),
            harness_session_id: None,
            harness_session_cwd: None,
            space_id: Some("space".into()),
            last_seen_at: None,
            room_gen: None,
        }
    }

    fn space(device: &str) -> Space {
        Space {
            id: "space".into(),
            device_id: device.into(),
            path: "/project".into(),
            name: None,
            git_detected: true,
            git_checked_at: None,
            checkout_id: Some("checkout".into()),
            created_at: Utc.timestamp_opt(0, 0).unwrap(),
        }
    }

    fn snapshot(device: &str, cwd: &str, checkout: &str) -> CheckoutChangeRequestStatus {
        CheckoutChangeRequestStatus {
            checkout_id: checkout.into(),
            device_id: device.into(),
            cwd: cwd.into(),
            branch: "feature/pr".into(),
            change_request: Some(ChangeRequestSummary {
                provider: "github".into(),
                number: 90,
                title: "Add pull request badges".into(),
                url: "https://github.com/acme/zeron/pull/90".into(),
                state: ChangeRequestState::Open,
                base_ref: "main".into(),
                head_ref: "feature/pr".into(),
            }),
            updated_at: Utc.timestamp_opt(1, 0).unwrap(),
        }
    }

    fn with_source(mut chat: Chat, branch: &str) -> Chat {
        chat.source_context = Some(zeron_proto::ConversationSourceContext {
            checkout_id: "checkout".into(),
            repo_root: "/repo".into(),
            cwd: "/repo".into(),
            branch: branch.into(),
            head_sha: Some("abc123".into()),
            observed_at: Utc.timestamp_opt(2, 0).unwrap(),
        });
        chat
    }

    fn workers_project(id: &str, branch: Option<&str>) -> zeron_workers_unpeel::WorkersProject {
        zeron_workers_unpeel::WorkersProject {
            id: id.to_owned(),
            name: id.to_owned(),
            path: format!("/repos/{id}"),
            folder_id: None,
            parent_project_id: None,
            is_group: false,
            worktree_branch: branch.map(str::to_owned),
            git_branch: Some("fix/renamed".into()),
            archived_session_count: 0,
            folder_color_id: None,
            session_sort: zeron_workers_unpeel::WorkersSessionSort::Custom,
        }
    }

    #[test]
    fn workers_change_request_targets_include_local_checkouts_and_skip_groups() {
        let mut group = workers_project("group", None);
        group.is_group = true;
        let mut unknown = workers_project("unknown", None);
        unknown.git_branch = None;
        let projects = [
            workers_project("plain", None),
            workers_project("wt", Some("old")),
            group,
            unknown,
        ];
        let targets = workers_change_request_targets(&projects, "local");
        assert_eq!(targets.len(), 2);
        assert!(
            targets
                .iter()
                .any(|target| target.cwd == "/repos/plain" && target.branch == "fix/renamed")
        );
        assert!(
            targets
                .iter()
                .any(|target| target.cwd == "/repos/wt" && target.branch == "fix/renamed")
        );
    }

    #[test]
    fn change_request_for_checkout_requires_cwd_and_branch() {
        let stored = snapshot("local", "/repos/wt", "checkout");
        assert_eq!(
            change_request_for_checkout("/repos/wt", "feature/pr", [&stored]).map(|pr| pr.number),
            Some(90)
        );
        // The worktree moved to another branch: the snapshot left behind names
        // the PR of the branch it USED to be on.
        assert!(change_request_for_checkout("/repos/wt", "fix/other", [&stored]).is_none());
        assert!(change_request_for_checkout("/repos/other", "feature/pr", [&stored]).is_none());
        assert!(change_request_for_checkout("/repos/wt", "  ", [&stored]).is_none());
    }

    /// The host reports the symlink-resolved toplevel (`/private/var/...`),
    /// an adopted worktree keeps the path the user gave (`/var/...`).
    #[cfg(unix)]
    #[test]
    fn change_request_for_checkout_matches_across_symlinks() {
        let temp = tempfile::tempdir().unwrap();
        let resolved = temp.path().join("private/wt");
        std::fs::create_dir_all(&resolved).unwrap();
        std::os::unix::fs::symlink(temp.path().join("private"), temp.path().join("var")).unwrap();
        let linked = temp.path().join("var/wt");

        let stored = snapshot("local", resolved.to_str().unwrap(), "checkout");
        assert_eq!(
            change_request_for_checkout(linked.to_str().unwrap(), "feature/pr", [&stored])
                .map(|pr| pr.number),
            Some(90)
        );
        // A different directory under the same symlinked parent still misses.
        // It has to EXIST, or the miss proves nothing: canonicalization of a
        // missing path fails and the comparison never runs.
        std::fs::create_dir_all(temp.path().join("private/other")).unwrap();
        assert!(
            change_request_for_checkout(
                temp.path().join("var/other").to_str().unwrap(),
                "feature/pr",
                [&stored]
            )
            .is_none()
        );
    }

    /// The lookup runs per row per frame, so the blocking `realpath` must stay
    /// behind the cheap scalar tests — and behind the shared cell.
    #[test]
    fn chat_lookup_keeps_canonicalization_off_the_render_path() {
        let chat = with_source(
            chat("chat", "local", Some("/repo"), Some("checkout")),
            "feature/pr",
        );
        let mut other_branch = snapshot("local", "/elsewhere", "checkout");
        other_branch.branch = "feature/other".into();
        let mut other_checkout = snapshot("local", "/elsewhere", "other-checkout");
        other_checkout.branch = "feature/pr".into();
        let other_device = snapshot("remote", "/elsewhere", "checkout");

        // Device, branch and checkout each reject on their own: no syscall.
        CANONICALIZE_CALLS.with(|calls| calls.set(0));
        assert!(
            change_request_for_chat(&chat, &[], [&other_branch, &other_checkout, &other_device])
                .is_none()
        );
        assert_eq!(CANONICALIZE_CALLS.with(|calls| calls.get()), 0);

        // Snapshots that do survive the cheap tests canonicalize `cwd` once,
        // not once per snapshot — and `/repo` not existing ends it there.
        let first = snapshot("local", "/a", "checkout");
        let second = snapshot("local", "/b", "checkout");
        let third = snapshot("local", "/c", "checkout");
        CANONICALIZE_CALLS.with(|calls| calls.set(0));
        assert!(change_request_for_chat(&chat, &[], [&first, &second, &third]).is_none());
        assert_eq!(CANONICALIZE_CALLS.with(|calls| calls.get()), 1);
    }

    #[test]
    fn shared_checkout_keeps_conversation_branches_independent() {
        let first = with_source(chat("first", "local", Some("/repo"), None), "feature/one");
        let second = with_source(chat("second", "local", Some("/repo"), None), "feature/two");
        let targets = desired_watch_targets(&[first.clone(), second.clone()], &[], |_| false);
        assert_eq!(targets.len(), 2);
        assert!(targets.iter().any(|target| target.branch == "feature/one"));
        assert!(targets.iter().any(|target| target.branch == "feature/two"));

        let mut first_snapshot = snapshot("local", "/repo", "checkout");
        first_snapshot.branch = "feature/one".into();
        assert_eq!(
            change_request_for_chat(&first, &[], [&first_snapshot]).map(|pr| pr.number),
            Some(90)
        );
        assert!(change_request_for_chat(&second, &[], [&first_snapshot]).is_none());
    }

    #[test]
    fn source_context_chat_finds_local_snapshot() {
        let chat = with_source(
            chat("chat", "local", Some("/repo"), Some("checkout")),
            "feature/pr",
        );
        let status = snapshot("local", "/repo", "checkout");
        assert_eq!(
            change_request_for_chat(&chat, &[], [&status]).map(|pr| pr.number),
            Some(90)
        );
    }

    #[test]
    fn remote_chat_requires_the_same_device() {
        let chat = with_source(
            chat("chat", "remote", Some("/repo"), Some("checkout")),
            "feature/pr",
        );
        let status = snapshot("local", "/repo", "checkout");
        assert!(change_request_for_chat(&chat, &[], [&status]).is_none());
    }

    #[test]
    fn mismatched_checkout_rejects_cwd_match() {
        let mut chat = with_source(
            chat("chat", "local", Some("/repo"), Some("new-checkout")),
            "feature/pr",
        );
        chat.source_context.as_mut().unwrap().checkout_id = "new-checkout".into();
        let status = snapshot("local", "/repo", "old-checkout");
        assert!(change_request_for_chat(&chat, &[], [&status]).is_none());
    }

    #[test]
    fn source_less_worktree_hides_stale_legacy_metadata() {
        let chat = chat("chat", "local", Some("/repo"), None);
        let status = snapshot("local", "/repo", "checkout");
        assert!(change_request_for_chat(&chat, &[], [&status]).is_none());
        assert!(conversation_branch(&chat, &[]).is_none());
        assert!(desired_watch_targets(&[chat], &[], |_| false).is_empty());
    }

    #[test]
    fn source_less_project_root_hides_legacy_branch_metadata() {
        let chat = chat("chat", "local", None, None);
        let status = snapshot("local", "/project", "checkout");
        assert!(change_request_for_chat(&chat, &[space("local")], [&status]).is_none());
        assert!(desired_watch_targets(&[chat], &[space("local")], |_| false).is_empty());
    }

    #[test]
    fn different_branch_hides_snapshot() {
        let mut chat = with_source(
            chat("chat", "local", Some("/repo"), Some("checkout")),
            "feature/other",
        );
        chat.branch = Some("feature/pr".into());
        let status = snapshot("local", "/repo", "checkout");
        assert!(change_request_for_chat(&chat, &[], [&status]).is_none());
    }

    #[test]
    fn shared_chats_deduplicate_and_last_archive_removes_target() {
        let first = with_source(
            chat("one", "local", Some("/repo"), Some("checkout")),
            "feature/pr",
        );
        let mut second = with_source(
            chat("two", "local", Some("/repo"), Some("checkout")),
            "feature/pr",
        );
        let targets = desired_watch_targets(&[first.clone(), second.clone()], &[], |_| false);
        assert_eq!(targets.len(), 1);

        second.archived = true;
        let targets = desired_watch_targets(&[first.clone(), second.clone()], &[], |_| false);
        assert_eq!(targets.len(), 1);

        let mut first = first;
        first.archived = true;
        assert!(desired_watch_targets(&[first, second], &[], |_| false).is_empty());
    }

    #[test]
    fn unsupported_device_has_no_targets_or_visible_snapshot() {
        let chat = with_source(
            chat("chat", "old-engine", Some("/repo"), Some("checkout")),
            "feature/pr",
        );
        let mut state = ChangeRequestClientState::default();
        state.store(
            ChangeRequestWatchKey {
                device_id: "old-engine".into(),
                cwd: "/repo".into(),
                branch: "feature/pr".into(),
                checkout_id: Some("checkout".into()),
            },
            snapshot("old-engine", "/repo", "checkout"),
        );
        state.mark_unsupported("old-engine".into(), Some("0.2.2".into()));

        assert!(state.change_request_for_chat(&chat, &[]).is_none());
        assert!(
            desired_watch_targets(&[chat], &[], |device| !state.is_supported(device)).is_empty()
        );
    }

    #[test]
    fn host_upgrade_reenables_a_previously_unsupported_device() {
        let chat = with_source(
            chat("chat", "host", Some("/repo"), Some("checkout")),
            "feature/pr",
        );
        let mut state = ChangeRequestClientState::default();
        state.mark_unsupported("host".into(), Some("0.2.2".into()));

        assert!(!state.is_supported("host"));
        assert!(!state.clear_unsupported_on_version_change("host", Some("0.2.2")));
        assert!(state.clear_unsupported_on_version_change("host", Some("0.2.3")));
        assert!(state.is_supported("host"));
        assert_eq!(
            desired_watch_targets(&[chat], &[], |device| !state.is_supported(device)).len(),
            1
        );
    }

    #[test]
    fn successful_none_clears_a_previous_pr() {
        let chat = with_source(
            chat("chat", "local", Some("/repo"), Some("checkout")),
            "feature/pr",
        );
        let key = ChangeRequestWatchKey {
            device_id: "local".into(),
            cwd: "/repo".into(),
            branch: "feature/pr".into(),
            checkout_id: Some("checkout".into()),
        };
        let mut state = ChangeRequestClientState::default();
        state.store(key.clone(), snapshot("local", "/repo", "checkout"));
        assert!(state.change_request_for_chat(&chat, &[]).is_some());

        let mut none = snapshot("local", "/repo", "checkout");
        none.change_request = None;
        state.store(key, none);
        assert!(state.change_request_for_chat(&chat, &[]).is_none());
    }

    #[test]
    fn local_and_remote_watch_params_route_to_the_checkout_host() {
        let target = ChangeRequestWatchKey {
            device_id: "host".into(),
            cwd: "/repo".into(),
            branch: "feature/pr".into(),
            checkout_id: Some("checkout".into()),
        };
        assert_eq!(
            watch_params(&target, Some("host")),
            serde_json::json!({
                "cwd": "/repo",
                "branch": "feature/pr"
            })
        );
        assert_eq!(
            watch_params(&target, Some("viewport")),
            serde_json::json!({
                "cwd": "/repo",
                "branch": "feature/pr",
                "targetDeviceId": "host"
            })
        );
    }

    #[test]
    fn badge_models_cover_open_merged_and_closed() {
        let cases = [
            (
                ChangeRequestState::Open,
                "Open",
                ChangeRequestBadgeTone::Open,
            ),
            (
                ChangeRequestState::Merged,
                "Merged",
                ChangeRequestBadgeTone::Merged,
            ),
            (
                ChangeRequestState::Closed,
                "Closed",
                ChangeRequestBadgeTone::Closed,
            ),
        ];
        for (state, label, tone) in cases {
            let mut summary = snapshot("local", "/repo", "checkout")
                .change_request
                .unwrap();
            summary.state = state;
            summary.title = "First line\nSecond line".into();
            let model = ChangeRequestBadgeModel::from_summary(&summary);
            assert_eq!(model.number, "#90");
            assert_eq!(model.state_label, label);
            assert_eq!(model.tone, tone);
            assert_eq!(model.title, "First line Second line");
        }
    }
}
