//! Process file-descriptor ceiling.
//!
//! macOS (and some Linux distros) start GUI-launched processes with a soft
//! `RLIMIT_NOFILE` of 256. Boot recovery used to open every journaled chat's
//! room at once; that stampede exhausted the ceiling (`EMFILE`), after which
//! Metal aborted loading MPSImage's `default.metallib` — `cargo run` died
//! with `zsh: abort` (2026-09-11). Raising the soft limit to the hard max
//! is the same first-line defense Chrome/Zed apply. Dial backpressure and
//! peek-before-open live elsewhere; this only lifts the process ceiling.

/// Raise the soft `RLIMIT_NOFILE` up to the hard max. Idempotent. No-op on
/// non-Unix, or when the ceiling is already at the hard max, or when the
/// kernel refuses the raise (the dial cap still bounds the stampede).
pub fn raise_nofile_limit() {
    #[cfg(unix)]
    {
        use std::sync::Once;
        static ONCE: Once = Once::new();
        ONCE.call_once(raise_unix_nofile_limit);
    }
}

#[cfg(unix)]
fn raise_unix_nofile_limit() {
    let mut lim = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };
    // SAFETY: `getrlimit`/`setrlimit` with a stack `rlimit` for this process.
    let rc = unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, &raw mut lim) };
    if rc != 0 {
        tracing::warn!("getrlimit(RLIMIT_NOFILE) failed; leaving process ceiling unchanged");
        return;
    }
    let soft = lim.rlim_cur;
    let mut want = lim.rlim_max;
    if want == libc::RLIM_INFINITY {
        // 10240 is Darwin OPEN_MAX — a documented, unprivileged ceiling.
        want = 10_240;
    }
    if want <= soft {
        return;
    }
    lim.rlim_cur = want;
    let rc = unsafe { libc::setrlimit(libc::RLIMIT_NOFILE, &raw const lim) };
    if rc != 0 {
        tracing::warn!(
            soft,
            hard = lim.rlim_max,
            "setrlimit(RLIMIT_NOFILE) refused; leaving process ceiling unchanged"
        );
        return;
    }
    tracing::info!(from = soft, to = want, "raised RLIMIT_NOFILE");
}

#[cfg(all(test, unix))]
fn current_nofile_soft() -> libc::rlim_t {
    let mut lim = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };
    let rc = unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, &raw mut lim) };
    assert_eq!(rc, 0, "getrlimit(RLIMIT_NOFILE)");
    lim.rlim_cur
}

#[cfg(unix)]
#[test]
fn raise_nofile_limit_never_lowers_the_soft_ceiling() {
    let before = current_nofile_soft();
    raise_nofile_limit();
    let after = current_nofile_soft();
    assert!(
        after >= before,
        "soft RLIMIT_NOFILE dropped from {before} to {after}"
    );
}
