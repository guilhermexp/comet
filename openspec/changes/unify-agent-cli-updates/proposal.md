# Change: One agent CLI updater — upstream's, with the fork's reach

## Why

Two updaters watched the same CLIs. Upstream #389 ships the Home update island and `HarnessUpdateCoordinator` (engine, per device, with idle leases). The fork's `worker-cli-maintenance` added a second one: `workers-unpeel::maintenance`, a startup "N provider updates available" toast and version badges in Workers Presets. The two disagreed. For example, Codex installed through npm showed "View steps" in the island and "Update all" in the toast. The fork also had to keep OMP out of the coordinator so the two would not race.

The owner decided to keep upstream's updater, move the fork's features into it and delete the fork's copy.

## What Changes

- **Engine (`crates/engine/src/harness_updates.rs`)**
  - Codex installed through npm (`<prefix>/lib/node_modules/@openai/codex`) updates with that prefix's own `bin/npm install -g --prefix <prefix> @openai/codex@latest`.
  - Codex installed through the Homebrew cask updates with the `brew` beside the Caskroom: `upgrade --cask codex`.
  - Anything else stays manual.
  - OMP is monitored: npm `@oh-my-pi/pi-coding-agent` gives the latest version, `omp --no-extensions --version` gives the installed one, and `omp update` installs.
  - Kimi stays unmonitored.
  - Pi updates with `pi update --all` (pi plus its packages) instead of upstream's `--self`.
- **Home island (`crates/ui/src/shell/harness_updates.rs`)**: when two or more rows are connected, `Available` and applicable, the summary shows **Update all**, which sends `APPLY_HARNESS_UPDATE` for each.
- **Workers Presets (`crates/ui/src/workers/{model,settings}.rs`)**
  - The version delta, **Update**, manual-command copy, **Update all** and **Check for updates** now read the local `AppState::harness_updates` and call the engine RPCs.
  - `agy` (the Antigravity CLI) has no engine harness, so it shows no badge. The owner accepted that.
- **Removed**
  - `crates/workers-unpeel/src/maintenance.rs` and its exports;
  - the startup advisory check and the provider-update toast (`ToastKind::ProviderUpdate`/`UpdatingProgress`, `ToastAction`, the toast action buttons);
  - `Shell::run_update_all_worker_clis`;
  - the `reqwest` dependency of `workers-unpeel`.
- Supersedes the unarchived change `worker-cli-maintenance`, which is deleted. Its history stays in git.

## Impact

- Affected: `crates/engine/src/harness_updates.rs`, `crates/ui/src/shell/harness_updates.rs`, `crates/ui/src/shell.rs`, `crates/ui/src/toast.rs`, `crates/ui/src/workers/{model,settings}.rs`, `crates/workers-unpeel/{src/lib.rs,Cargo.toml}`.
- No wire changes. `HarnessUpdateStatus` already carries everything.
- The fork's startup toast is replaced by upstream's island plus its OS notification (`agent_update_notifications`).
