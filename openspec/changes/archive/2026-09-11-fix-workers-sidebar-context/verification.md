# Verification — 2026-09-11

- RED: `change_request_branch_uses_current_checkout_including_local_branches` failed with `None` instead of `Some("main")` before the resolver fix.
- RED: `empty_projects_are_not_selected_implicitly_after_refresh` failed with `Some("craft")` instead of `None` before removing the registry fallback.
- Focused checks: Workers UI (182 passed) and change-request projection (17 passed).
- Native GPUI demo, isolated `UNPEEL_HOME` on IPC 27939: CUA screenshot showed `Local · main` and `Worktree · feature/sidebar`, no repeated session Git glyph, and no row for an empty Craft fixture placed first in the registry. Actual temporary Git checkout/worktree provided branch metadata.
- Native scope: offline demo; no live GitHub PR badge/provider verification and no archive action against the user's Workers. Last-session selection behavior is covered by unit regressions; native evidence covers the layout and initial empty-project visibility.
- Demo processes were stopped after inspection. User app and Worker session hosts were not restarted or terminated.
- DOX owners updated; no new ownership boundary or child index change.
- OpenSpec change strict validation and all 45 main specs passed.
- Full gate: `cargo test --workspace -- --test-threads=1` exited 0; 2721 passed, 0 failed, 18 ignored across 83 summaries. Log: `/tmp/comet-workers-sidebar-workspace-20260911.log`.
- Final `cargo build -p zeron` exited 0; local `target/debug/zeron` updated. Log: `/tmp/comet-workers-sidebar-build-20260911.log`.
- `git diff --check` passed; no commit or push.
