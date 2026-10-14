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
fn pause_hook_wait_hook_review_detail_footer_is_a_menu() {
    let screen = "\
             SessionStart hooks\n\
             1 hook needs review before it can run.\n\
             › [!]  1  Unnamed hook · new\n\
             Event     SessionStart\n\
             Source    User config - /private/tmp/cwg-j6y27kk6/h/.codex/hooks.json\n\
             Command   /usr/bin/true\n\
             Mode      Sync\n\
             Timeout   600s\n\
             Trust     New hook - review required\n\
             t trust · esc back";
    assert_eq!(viewport_has_menu_prompt(screen), true);
    assert_eq!(viewport_has_menu_prompt("  t trust · esc back   "), true);
    assert_eq!(viewport_has_menu_prompt("T TRUST · ESC BACK"), true);
}

#[test]
fn pause_hook_wait_trusted_detail_and_trust_prose_are_not_menus() {
    let trusted = "\
             SessionStart hooks\n\
             1 hook needs review before it can run.\n\
             Command   /usr/bin/true\n\
             Trust     Trusted";
    assert_eq!(viewport_has_menu_prompt(trusted), false);
    let warning_with_label = "\
             SessionStart hooks\n\
             1 hook needs review before it can run.\n\
             Command   /usr/bin/true\n\
             Trust     New hook - review required";
    assert_eq!(viewport_has_menu_prompt(warning_with_label), false);
    assert_eq!(
        viewport_has_menu_prompt("Please trust this hook, then go back to the list."),
        false
    );
    assert_eq!(viewport_has_menu_prompt("do not trust · esc back"), false);
}

#[test]
fn pause_hook_wait_quoted_footer_in_log_is_not_a_menu() {
    assert_eq!(
        viewport_has_menu_prompt("Log: \"t trust · esc back\""),
        false
    );
}

#[test]
fn pause_hook_wait_unknown_footer_suffix_is_not_a_menu() {
    assert_eq!(viewport_has_menu_prompt("t trust · esc backwards"), false);
}

#[test]
fn pause_hook_wait_unicode_prefix_is_not_a_menu() {
    assert_eq!(viewport_has_menu_prompt("ét trust · esc back"), false);
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
