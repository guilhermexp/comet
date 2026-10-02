## 1. Engine
- [x] 1.1 Codex package-manager update plan (npm prefix, Homebrew cask), manual otherwise
- [x] 1.2 Monitor OMP (npm latest, `omp update`); Kimi stays out
- [x] 1.3 Tests: `codex_updates_through_the_package_manager_that_owns_the_install`, `omp_is_tracked_and_updates_itself`

## 2. UI
- [x] 2.1 Home island **Update all** for ≥2 applicable rows (`update_all_installs_only_connected_applicable_rows`)
- [x] 2.2 Workers Presets read `AppState::harness_updates` and call the engine RPCs (`presets_map_their_cli_to_the_upstream_updater_row`)
- [x] 2.3 Remove the startup advisory toast and toast action plumbing

## 3. Removal
- [x] 3.1 Delete `workers-unpeel::maintenance`, its exports and the `reqwest` dependency
- [x] 3.2 Delete the superseded `worker-cli-maintenance` change
- [x] 3.3 DOX pass (`crates/engine`, `crates/ui`, `crates/workers-unpeel` AGENTS.md)

## 4. Validation
- [ ] 4.1 Visual: island shows **Update** for npm Codex and **Update all** with two updates; Presets badges follow the island
