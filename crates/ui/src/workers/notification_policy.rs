#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerNotification {
    Attention,
    Done,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationState {
    pub generation: u64,
    pub activity: String,
    pub unread: bool,
    pub attention_sent: bool,
    pub done_sent: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct NotificationSample<'a> {
    pub generation: u64,
    pub activity: &'a str,
    pub unread: bool,
    pub notify_when_done: bool,
}

pub fn reduce_notification(
    previous: Option<&NotificationState>,
    sample: NotificationSample<'_>,
) -> (NotificationState, Option<WorkerNotification>) {
    let Some(previous) = previous else {
        return (
            NotificationState {
                generation: sample.generation,
                activity: sample.activity.to_owned(),
                unread: sample.unread,
                attention_sent: sample.activity == "blocked",
                done_sent: sample.activity == "done" && sample.unread,
            },
            None,
        );
    };
    if sample.generation < previous.generation {
        return (previous.clone(), None);
    }

    let next_generation = sample.generation > previous.generation;
    let attention_edge = sample.activity == "blocked"
        && (next_generation || previous.activity != "blocked")
        && (next_generation || !previous.attention_sent);
    let done_edge = sample.activity == "done"
        && sample.unread
        && (next_generation || previous.activity != "done" || !previous.unread)
        && (next_generation || !previous.done_sent);

    let state = NotificationState {
        generation: sample.generation,
        activity: sample.activity.to_owned(),
        unread: sample.unread,
        attention_sent: if next_generation {
            attention_edge
        } else {
            previous.attention_sent || attention_edge
        },
        done_sent: if next_generation {
            done_edge
        } else {
            previous.done_sent || done_edge
        },
    };
    let event = if attention_edge {
        Some(WorkerNotification::Attention)
    } else if done_edge && sample.notify_when_done {
        Some(WorkerNotification::Done)
    } else {
        None
    };
    (state, event)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(
        generation: u64,
        activity: &'static str,
        unread: bool,
        notify_when_done: bool,
    ) -> NotificationSample<'static> {
        NotificationSample {
            generation,
            activity,
            unread,
            notify_when_done,
        }
    }

    #[test]
    fn initial_snapshot_seeds_without_notifying() {
        let (_, event) = reduce_notification(None, sample(1, "blocked", false, true));
        assert_eq!(event, None);
    }

    #[test]
    fn attention_is_one_rising_edge_per_generation() {
        let (idle, _) = reduce_notification(None, sample(7, "idle", false, false));
        let (attention, event) =
            reduce_notification(Some(&idle), sample(7, "blocked", false, false));
        assert_eq!(event, Some(WorkerNotification::Attention));

        let (_, duplicate) =
            reduce_notification(Some(&attention), sample(7, "blocked", false, false));
        assert_eq!(duplicate, None);
    }

    #[test]
    fn done_requires_session_opt_in_and_is_deduplicated() {
        let (working, _) = reduce_notification(None, sample(3, "working", false, false));
        let (_, disabled) = reduce_notification(Some(&working), sample(3, "done", true, false));
        assert_eq!(disabled, None);

        let (working, _) = reduce_notification(None, sample(4, "working", false, true));
        let (done, enabled) = reduce_notification(Some(&working), sample(4, "done", true, true));
        assert_eq!(enabled, Some(WorkerNotification::Done));
        let (_, duplicate) = reduce_notification(Some(&done), sample(4, "done", true, true));
        assert_eq!(duplicate, None);
    }

    #[test]
    fn stale_generation_cannot_change_state_or_notify() {
        let current = NotificationState {
            generation: 9,
            activity: "working".into(),
            unread: false,
            attention_sent: false,
            done_sent: false,
        };
        let (state, event) = reduce_notification(Some(&current), sample(8, "blocked", false, true));
        assert_eq!(state, current);
        assert_eq!(event, None);
    }

    #[test]
    fn next_generation_rearms_both_edges() {
        let previous = NotificationState {
            generation: 5,
            activity: "done".into(),
            unread: true,
            attention_sent: true,
            done_sent: true,
        };
        let (state, event) =
            reduce_notification(Some(&previous), sample(6, "blocked", false, true));
        assert_eq!(event, Some(WorkerNotification::Attention));
        assert_eq!(state.generation, 6);
        assert!(state.attention_sent);
        assert!(!state.done_sent);
    }
}
