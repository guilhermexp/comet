# Verification

Use `.tmp/verify/spinner-stop-20260930/` for evidence and private session fixtures; `.tmp` must be Git-ignored. Do not run failure injection against the owner's profile.

1. Capture RED from the persisted-Stop recovery and seventeen-listener tests before changing behavior.
2. Run shared activity tests (compiled directly from the canonical source using the existing dependency cache for a cheap initial RED/GREEN); then run the actual downstream `zeron-workers-unpeel` suite through `scripts/cargo-verify.py` once the current unrelated Cargo verification releases its lock. Exercise runtime-generation, live-newer-than-disk, unchanged seeds and Codex rearm controls.
3. The functional fixture starts with a working Claude-style Worker state and private session files. Persist Stop without delivering its POST; the next real Comet activity derivation must become idle immediately. Bind seventeen disposable loopback listeners, register and unregister the last, and inspect persisted ports rather than a self-reported success flag.
4. Build the app and, if a headed isolated launch is feasible, use private HOME/UNPEEL_HOME/ZERON_DATA_DIR/COMET_WORKERS_HOOKS_DIR and disabled sync. Observe the spinner before/after a controlled turn. Record substitution if only state derivation was exercised; unit tests do not prove native rendering.
5. Review diff, check formatting/specs, preserve all evidence, and close only this run's processes. The existing user app and Worker processes remain owned by their sessions.

Observed substitute while Cargo was unavailable: compile the actual adapter `src/lib.rs` directly with `rustc --test --edition=2024`, selecting mutually compatible libraries from the existing cache by Cargo fingerprints. The focused activity subset passed 48/48 and the complete unit binary passed 289/289. This verifies current adapter source and canonical included activity; it does not replace Cargo dependency resolution/integration suites, a full app build or native rendering. An additional isolated fixture compiles exact production `derive_activity` and `session_indicator` definitions, proving working → idle/unread after a persisted Stop without HTTP; RED against the prior activity implementation retained working.

The wrapper eventually refused with `Cargo verification refused: 57.5 GiB free; requires 60 GiB before starting.` Preserve that disk gate. Free space through an authorized maintenance action, then run steps 2 and 4; do not remove another task's caches or lower the threshold to claim completion.
