use std::time::SystemTime;

use super::super::{derive_activity, upstream_activity::ActivityEngine};
use super::input;

#[test]
fn pause_hook_wait_settled_interrupt_drops_working_spinner_without_confirming_completion() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(
        directory.path().join("last-hook-event.json"),
        r#"{"hook_event_name":"Start"}"#,
    )
    .unwrap();
    let mut engine = ActivityEngine::default();
    let now = SystemTime::now();
    assert_eq!(
        derive_activity(&mut engine, input("s1", "codex", directory.path(), 1), now,),
        Some("working")
    );

    let mut interrupted = input("s1", "codex", directory.path(), 2);
    interrupted.turn_settled_by_interrupt = true;
    assert_eq!(
        derive_activity(&mut engine, interrupted, now),
        Some("idle"),
        "a settled interrupt must drop the spinner"
    );
    assert!(
        !engine.hook_confirmed_idle("s1"),
        "interrupt must not confirm Stop or complete the episode"
    );

    assert_eq!(
        derive_activity(&mut engine, input("s1", "codex", directory.path(), 3), now,),
        Some("working"),
        "clearing the interrupt evidence must rearm the live turn"
    );
}

#[test]
fn pause_hook_wait_closing_review_menu_restores_hook_state() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(
        directory.path().join("last-hook-event.json"),
        r#"{"hook_event_name":"Start"}"#,
    )
    .unwrap();
    let mut engine = ActivityEngine::default();
    let now = SystemTime::now();
    let mut open = input("s1", "codex", directory.path(), 1);
    open.menu_prompt_active = true;
    assert_eq!(derive_activity(&mut engine, open, now), Some("blocked"));

    assert_eq!(
        derive_activity(&mut engine, input("s1", "codex", directory.path(), 2), now,),
        Some("working")
    );
}
