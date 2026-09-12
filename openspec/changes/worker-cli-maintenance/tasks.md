# Tasks: Worker CLI Maintenance and Startup Update Notifications

## 1. Specification and Design
- [x] 1.1 Author proposal.md, design.md, spec.md, and tasks.md
- [x] 1.2 Validate change with `openspec validate worker-cli-maintenance --strict --no-interactive`

## 2. Core Maintenance in `crates/workers-unpeel`
- [x] 2.1 Implement `crates/workers-unpeel/src/maintenance.rs` with runtime catalog definitions, semver parsing/comparison, install source detection, and npm/brew version querying with 1h TTL cache
- [x] 2.2 Implement safe update command execution with per-CLI concurrency lock, 5min timeout, and post-update re-verification
- [x] 2.3 Expose maintenance methods (`check_advisories`, `run_update`) on `LocalWorkersClient` / `WorkersModel`
- [x] 2.4 Add unit tests covering semver comparison, install source detection, advisory construction, and update execution

## 3. Bottom-Right Toast Overlay in `crates/ui`
- [x] 3.1 Implement `crates/ui/src/toast.rs` supporting floating bottom-right toasts with custom message, action buttons (`Review`, `Update all`), progress state, and auto-dismiss timer (10s)
- [x] 3.2 Wire toast rendering in `Shell::render_overlays`
- [x] 3.3 Add unit tests for toast management and dismissal lifecycle

## 4. Workers Presets Settings UI
- [x] 4.1 Update `crates/ui/src/workers/settings.rs` to display version advisories on preset rows (`Current vX` or `vX → vY` with `Update` button)
- [x] 4.2 Add header controls: `Update all` button and `Check for updates` refresh button
- [x] 4.3 Support in-place loading/updating spinner during update runs

## 5. Startup Check and Notification Integration
- [x] 5.1 Wire non-blocking background startup version check in `Shell` / `WorkersModel`
- [x] 5.2 Implement per-session deduplication flag (`SHOWN_THIS_SESSION`)
- [x] 5.3 Show bottom-right startup update toast when updates are detected

## 6. Verification and Cleanup
- [x] 6.1 Run cargo check and unit tests
- [x] 6.2 Format with `cargo fmt --all`
- [x] 6.3 Verify OpenSpec strict validation passes
