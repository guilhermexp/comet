//! One local, drop-cancelled subscription for visible Worked Project paths.
use super::*;

impl DetailsSidebar {
    pub(crate) fn hide_worked_project_diffs(&mut self, cx: &mut Context<Self>) {
        self.hide_project_skill_usage();
        self.ensure_worked_project_diff_watch(Vec::new(), cx);
        self.ensure_dev_server_watch(None, Vec::new(), cx);
    }

    pub(super) fn ensure_worked_project_diff_watch(
        &mut self,
        mut paths: Vec<String>,
        cx: &mut Context<Self>,
    ) {
        paths.sort();
        paths.dedup();
        if paths.is_empty() {
            self.worked_diff_watch = None;
            self.worked_diff_key = None;
            self.worked_diffs.clear();
            return;
        }
        let state = self.app_state.read(cx);
        let key = (state.local_device_id.clone(), paths);
        if self.worked_diff_key.as_ref() == Some(&key) {
            return;
        }
        self.worked_diff_watch = None;
        self.worked_diff_key = None;
        self.worked_diffs.clear();
        let Some(engine) = state.engine().cloned() else {
            return;
        };
        self.worked_diff_key = Some(key.clone());
        self.worked_diff_watch = Some(cx.spawn(async move |this, cx| {
            // WorkersModel's registry is local; the Chat's target device does
            // not own these paths. Scoped cancellation releases all pins even
            // if the stream never emits another frame.
            if let Ok(mut rx) = engine
                .client()
                .subscribe_scoped(
                    methods::WATCH_CHECKOUT_DIFFS,
                    serde_json::json!({"cwds": key.1}),
                )
                .await
            {
                let mut diffs = Vec::new();
                while let Some(value) = rx.recv().await {
                    if !crate::changes::apply_diff_frame(&mut diffs, value) {
                        continue;
                    }
                    let totals = diffs
                        .iter()
                        .filter(|diff| key.1.binary_search(&diff.cwd).is_ok())
                        .map(|diff| {
                            (
                                diff.cwd.clone(),
                                session_card::SessionDiffTotals {
                                    files: diff.files.len(),
                                    additions: diff.additions,
                                    deletions: diff.deletions,
                                },
                            )
                        })
                        .collect();
                    if this
                        .update(cx, |this, cx| {
                            if this.worked_diff_key.as_ref() == Some(&key)
                                && this.worked_diffs != totals
                            {
                                this.worked_diffs = totals;
                                cx.notify();
                            }
                        })
                        .is_err()
                    {
                        return;
                    }
                }
            }
            let _ = this.update(cx, |this, cx| {
                if this.worked_diff_key.as_ref() == Some(&key) {
                    this.worked_diff_key = None;
                    let had_visible_totals = this.worked_diffs.values().any(|diff| diff.files > 0);
                    this.worked_diffs.clear();
                    // An immediately failing retry with no visible totals must
                    // wait for another render, rather than driving a hot loop.
                    if had_visible_totals {
                        cx.notify();
                    }
                }
            });
        }));
    }
}
