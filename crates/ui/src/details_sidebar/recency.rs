//! Recently-changed tinting for the Files pane.
//!
//! The tree itself carries no timestamps — the scan reads `file_type` only, so
//! recency is event-sourced: a filesystem event marks a path with the instant
//! the event ARRIVED, never the file's mtime. Marks decay through three tiers
//! and are pruned once the last one expires, so an idle pane holds no state.
//!
//! Decay is evaluated at render against a wall clock rather than driven by
//! `with_animation`: gpui keys animation clocks by element id and replays them
//! from `t = 0` on remount, and file rows are rebuilt wholesale on every
//! rescan — an animated highlight would restart on every keystroke.

use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

/// Just changed.
pub const RECENCY_FRESH: Duration = Duration::from_secs(10);
/// Changed during this stretch of work.
pub const RECENCY_RECENT: Duration = Duration::from_secs(60);
/// Last tier; the mark is dropped past this age.
pub const RECENCY_FADING: Duration = Duration::from_secs(180);
/// How often the pane re-evaluates tiers and prunes expired marks.
pub const RECENCY_TICK: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RecencyLevel {
    Fading,
    Recent,
    Fresh,
}

/// Per-pane map of workspace-relative path → instant the change was observed.
#[derive(Debug, Default)]
pub struct FileRecency {
    marked: HashMap<String, Instant>,
}

impl FileRecency {
    /// Records a change. A later instant always wins, so a stale event
    /// arriving out of order cannot weaken a fresher mark.
    pub fn mark(&mut self, relative_path: impl Into<String>, at: Instant) {
        self.marked
            .entry(relative_path.into())
            .and_modify(|current| {
                if at > *current {
                    *current = at;
                }
            })
            .or_insert(at);
    }

    pub fn clear(&mut self) {
        self.marked.clear();
    }

    pub fn is_empty(&self) -> bool {
        self.marked.is_empty()
    }

    /// Drops marks past [`RECENCY_FADING`]; `true` when anything was removed.
    pub fn prune(&mut self, now: Instant) -> bool {
        let before = self.marked.len();
        self.marked
            .retain(|_, marked_at| now.saturating_duration_since(*marked_at) < RECENCY_FADING);
        self.marked.len() != before
    }

    /// Tier for a file row.
    pub fn level(&self, relative_path: &str, now: Instant) -> Option<RecencyLevel> {
        let age = now.saturating_duration_since(*self.marked.get(relative_path)?);
        if age < RECENCY_FRESH {
            Some(RecencyLevel::Fresh)
        } else if age < RECENCY_RECENT {
            Some(RecencyLevel::Recent)
        } else if age < RECENCY_FADING {
            Some(RecencyLevel::Fading)
        } else {
            None
        }
    }

    /// Tier for a directory row: the strongest tier among its descendants, so
    /// a collapsed folder still advertises that something under it changed.
    pub fn folder_level(&self, relative_path: &str, now: Instant) -> Option<RecencyLevel> {
        // Segment-aware: `src/` must not claim a change under `srcsibling/`.
        let prefix = format!("{relative_path}/");
        let mut strongest = None;
        for path in self.marked.keys() {
            if !path.starts_with(&prefix) {
                continue;
            }
            let level = self.level(path, now);
            if level > strongest {
                strongest = level;
            }
            if strongest == Some(RecencyLevel::Fresh) {
                break;
            }
        }
        strongest
    }

    pub fn row_level(
        &self,
        relative_path: &str,
        is_dir: bool,
        now: Instant,
    ) -> Option<RecencyLevel> {
        if is_dir {
            self.folder_level(relative_path, now)
        } else {
            self.level(relative_path, now)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::{FileRecency, RecencyLevel};

    fn recency(marks: &[(&str, Duration)], now: Instant) -> FileRecency {
        let mut recency = FileRecency::default();
        for (path, age) in marks {
            recency.mark(*path, now - *age);
        }
        recency
    }

    #[test]
    fn tiers_follow_the_age_of_the_mark() {
        let now = Instant::now();
        let recency = recency(
            &[
                ("fresh.txt", Duration::from_secs(3)),
                ("recent.txt", Duration::from_secs(30)),
                ("fading.txt", Duration::from_secs(120)),
                ("expired.txt", Duration::from_secs(300)),
            ],
            now,
        );

        assert_eq!(recency.level("fresh.txt", now), Some(RecencyLevel::Fresh));
        assert_eq!(recency.level("recent.txt", now), Some(RecencyLevel::Recent));
        assert_eq!(recency.level("fading.txt", now), Some(RecencyLevel::Fading));
        assert_eq!(recency.level("expired.txt", now), None);
        assert_eq!(recency.level("never-touched.txt", now), None);
    }

    #[test]
    fn tier_boundaries_are_inclusive_of_the_stronger_tier() {
        let now = Instant::now();
        let recency = recency(
            &[
                ("edge-fresh.txt", Duration::from_secs(10)),
                ("edge-recent.txt", Duration::from_secs(60)),
                ("edge-drop.txt", Duration::from_secs(180)),
            ],
            now,
        );

        assert_eq!(
            recency.level("edge-fresh.txt", now),
            Some(RecencyLevel::Recent)
        );
        assert_eq!(
            recency.level("edge-recent.txt", now),
            Some(RecencyLevel::Fading)
        );
        assert_eq!(recency.level("edge-drop.txt", now), None);
    }

    #[test]
    fn a_second_change_refreshes_the_mark() {
        let now = Instant::now();
        let mut recency = recency(&[("file.txt", Duration::from_secs(120))], now);
        assert_eq!(recency.level("file.txt", now), Some(RecencyLevel::Fading));

        recency.mark("file.txt", now);
        assert_eq!(recency.level("file.txt", now), Some(RecencyLevel::Fresh));

        // An older event must never weaken a newer mark.
        recency.mark("file.txt", now - Duration::from_secs(120));
        assert_eq!(recency.level("file.txt", now), Some(RecencyLevel::Fresh));
    }

    #[test]
    fn a_folder_takes_the_strongest_tier_beneath_it() {
        let now = Instant::now();
        let recency = recency(
            &[
                ("src/deep/fresh.rs", Duration::from_secs(2)),
                ("src/old.rs", Duration::from_secs(120)),
                ("srcsibling/other.rs", Duration::from_secs(2)),
            ],
            now,
        );

        assert_eq!(recency.folder_level("src", now), Some(RecencyLevel::Fresh));
        assert_eq!(
            recency.folder_level("src/deep", now),
            Some(RecencyLevel::Fresh)
        );
        // Prefix matching is path-segment aware, not a raw string prefix.
        assert_eq!(recency.folder_level("sr", now), None);
        assert_eq!(
            recency.row_level("src", true, now),
            Some(RecencyLevel::Fresh)
        );
        assert_eq!(recency.row_level("src", false, now), None);
    }

    #[test]
    fn pruning_reports_change_and_empties_the_map() {
        let now = Instant::now();
        let mut recency = recency(
            &[
                ("stale.txt", Duration::from_secs(400)),
                ("live.txt", Duration::from_secs(5)),
            ],
            now,
        );

        assert!(recency.prune(now));
        assert!(!recency.prune(now));
        assert!(!recency.is_empty());
        assert_eq!(recency.level("stale.txt", now), None);

        recency.clear();
        assert!(recency.is_empty());
    }
}
