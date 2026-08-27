//! Background snapshot fetcher for the preview pane. Snapshots go through
//! `read_terminal_viewport_snapshot`, which asks the live host over its
//! control socket (virtual-client snapshot — never perturbs the grid desktop
//! attach or a phone owns) and falls back to a cached disk replay for dead
//! hosts. Either path can block on socket timeouts, so the UI thread never
//! fetches directly: it posts the wanted set each tick and one worker thread
//! publishes results into a shared map.

use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;

use unpeel_core::terminal_viewport::{read_terminal_viewport_snapshot, TerminalViewportSnapshot};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SnapshotRequest {
    pub session_id: String,
    pub cols: u16,
    pub rows: u16,
    /// Rows scrolled back from the live tail (0 = bottom).
    pub scroll_offset: u32,
}

pub struct SnapshotService {
    map: Arc<Mutex<HashMap<String, TerminalViewportSnapshot>>>,
    tx: Sender<Vec<SnapshotRequest>>,
}

impl SnapshotService {
    pub fn new() -> Self {
        let map: Arc<Mutex<HashMap<String, TerminalViewportSnapshot>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let (tx, rx): (Sender<Vec<SnapshotRequest>>, Receiver<Vec<SnapshotRequest>>) =
            mpsc::channel();
        let worker_map = Arc::clone(&map);
        thread::spawn(move || loop {
            let Ok(mut wanted) = rx.recv() else {
                return;
            };
            // Drain to the latest request set so a slow fetch never works
            // through a backlog of stale geometry.
            while let Ok(newer) = rx.try_recv() {
                wanted = newer;
            }
            for req in wanted {
                // Offset 0 — live hosts: the TRUE grid over the socket (no
                // reflow; the drawing layer letterboxes); dead hosts: disk
                // replay at the pane size. Scrolled back — always the disk
                // replay at the session's true grid: the host's live
                // viewport keeps no scrollback, but the replay VT holds the
                // full output history, so virtual scrolling works.
                let dir = unpeel_core::app_paths::app_sessions_root().join(&req.session_id);
                let result = if req.scroll_offset == 0 {
                    crate::control::viewport_snapshot(&dir, 0).or_else(|_| {
                        read_terminal_viewport_snapshot(
                            req.session_id.clone(),
                            req.cols.max(4),
                            req.rows.max(2),
                            None,
                            None,
                            None,
                        )
                    })
                } else {
                    let (cols, rows) = worker_map
                        .lock()
                        .ok()
                        .and_then(|g| g.get(&req.session_id).map(|s| (s.cols, s.rows)))
                        .unwrap_or((req.cols.max(4), req.rows.max(2)));
                    unpeel_core::terminal_viewport::replay_terminal_viewport_snapshot(
                        req.session_id.clone(),
                        cols,
                        rows,
                        None,
                        Some(req.scroll_offset),
                        None,
                    )
                };
                if let Ok(snapshot) = result {
                    if let Ok(mut guard) = worker_map.lock() {
                        guard.insert(req.session_id, snapshot);
                    }
                }
            }
        });
        SnapshotService { map, tx }
    }

    pub fn request(&self, wanted: Vec<SnapshotRequest>) {
        let _ = self.tx.send(wanted);
    }

    /// Locally rendered frame from the live streamed VT (`stream.rs`) — same
    /// map the drawing layer reads, so streamed and fetched sessions render
    /// through one path.
    pub fn publish(&self, session_id: String, snapshot: TerminalViewportSnapshot) {
        if let Ok(mut guard) = self.map.lock() {
            guard.insert(session_id, snapshot);
        }
    }

    pub fn get(&self, session_id: &str) -> Option<TerminalViewportSnapshot> {
        self.map.lock().ok()?.get(session_id).cloned()
    }
}
