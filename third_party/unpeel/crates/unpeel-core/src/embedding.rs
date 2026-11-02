//! Host-application embedding policy (Comet downstream).
//!
//! Comet embeds this crate behind its own binary and has retired the legacy
//! Browser (`agent-browser`) and Computer (cua-driver) engines: its Workers
//! use Comet's own Computer Use server and the providers' own browser tools.
//! The embedding registers that once per process, before any vendored entry
//! point runs. Standalone Unpeel never calls it, so its defaults and engines
//! are unchanged.

use std::sync::atomic::{AtomicBool, Ordering};

static LEGACY_ENGINES_RETIRED: AtomicBool = AtomicBool::new(false);

#[cfg(test)]
thread_local! {
    static TEST_OVERRIDE: std::cell::Cell<Option<bool>> = const { std::cell::Cell::new(None) };
}

/// Retire the legacy Browser and Computer domains for this process: launches
/// and resumes grant neither, and the MCP host neither advertises nor runs
/// them (not even `help`/`context`).
pub fn retire_legacy_engines() {
    LEGACY_ENGINES_RETIRED.store(true, Ordering::SeqCst);
}

pub fn legacy_engines_retired() -> bool {
    #[cfg(test)]
    if let Some(value) = TEST_OVERRIDE.with(std::cell::Cell::get) {
        return value;
    }
    LEGACY_ENGINES_RETIRED.load(Ordering::SeqCst)
}

/// Per-thread override so unit tests exercise both embeddings in parallel.
#[cfg(test)]
pub(crate) fn set_retired_for_test(value: Option<bool>) {
    TEST_OVERRIDE.with(|cell| cell.set(value));
}
