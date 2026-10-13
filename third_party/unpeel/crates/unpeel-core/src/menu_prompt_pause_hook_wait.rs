use super::super::{viewport_has_menu_prompt, viewport_shows_settled_interrupt};

#[test]
fn pause_hook_wait_codex_hooks_review_footer_is_a_menu_prompt() {
    let screen = "Hooks\n\
             1 hook needs review before it can run\n\
             \n\
             t trust all · enter review · esc close";
    assert_eq!(viewport_has_menu_prompt(screen), true);
}

#[test]
fn pause_hook_wait_hook_warning_prose_and_passive_list_are_not_menus() {
    assert_eq!(
        viewport_has_menu_prompt("1 hook needs review before it can run"),
        false
    );
    assert_eq!(
        viewport_has_menu_prompt(
            "Please review the hook output and tell me if the warning looks fine."
        ),
        false
    );
    assert_eq!(
        viewport_has_menu_prompt("Hooks\n  notify.sh\n  session_start.sh"),
        false
    );
}

#[test]
fn pause_hook_wait_settled_codex_interrupt_is_current_turn_evidence() {
    let settled = "codex\n\
             ■ Conversation interrupted\n\
             › ";
    assert_eq!(viewport_shows_settled_interrupt(settled), true);

    let resumed = "■ Conversation interrupted\n\
             › fix the spinner\n\
             Working (2s • esc to interrupt)";
    assert_eq!(viewport_shows_settled_interrupt(resumed), false);

    let background_only = "bash\n$\nsleep 30\n";
    assert_eq!(viewport_shows_settled_interrupt(background_only), false);
}
