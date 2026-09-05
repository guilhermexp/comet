## 1. Regression Coverage

- [x] 1.1 Do not retain an argv-only fake-OMP test; it does not prove extension discovery
- [x] 1.2 Keep `every_omp_launch_scopes_skills_to_the_project` as the automated skill-overlay regression

## 2. Session Launch

- [x] 2.1 Remove `--no-extensions` from `OmpProcess::start` and leave the remaining launch flags, skill overlay, and session semantics unchanged
- [x] 2.2 Confirm `every_omp_launch_scopes_skills_to_the_project` still passes

## 3. Contracts and Verification

- [x] 3.1 Update `crates/harness/AGENTS.md` so Session launch uses default extension discovery and the skill overlay remains independent
- [x] 3.2 Run `openspec validate enable-omp-extension-discovery --strict` and `cargo test -p zeron-harness`
- [x] 3.3 Smoke real `OmpProcess` against `/Users/guilhermevarela/.orchestrator`, record discovered `/fh-*` commands, then remove the throwaway
- [x] 3.4 Build the updated `zeron` binary without restarting the running app
- [x] 3.5 Archive the validated OpenSpec change
