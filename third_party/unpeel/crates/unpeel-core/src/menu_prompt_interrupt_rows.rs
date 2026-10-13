use super::super::viewport_shows_settled_interrupt;

#[test]
fn pause_hook_wait_interrupt_phrase_in_prose_or_quote_is_not_settled() {
    assert_eq!(
        viewport_shows_settled_interrupt(
            "codex\n\
             › \n\
             The log says Conversation interrupted but the turn is still running."
        ),
        false
    );
    assert_eq!(
        viewport_shows_settled_interrupt("> Conversation interrupted\n› "),
        false
    );
    assert_eq!(
        viewport_shows_settled_interrupt(
            "tool output: Conversation interrupted\n■ not the status row\n› "
        ),
        false
    );
}

#[test]
fn pause_hook_wait_prompt_only_above_interrupt_row_is_not_settled() {
    let screen = "› \n■ Conversation interrupted\ntool output still on screen";
    assert_eq!(viewport_shows_settled_interrupt(screen), false);
}

#[test]
fn pause_hook_wait_live_background_terminal_is_not_a_settled_interrupt() {
    let screen = "■ Conversation interrupted\nbash\n$\nsleep 30\n";
    assert_eq!(viewport_shows_settled_interrupt(screen), false);

    let settled_after_shell = "bash\n$\nsleep 30\n■ Conversation interrupted\n› ";
    assert_eq!(viewport_shows_settled_interrupt(settled_after_shell), true);
}

#[test]
fn pause_hook_wait_new_submission_rearms_before_working_footer_paints() {
    let screen = "■ Conversation interrupted\n› fix the spinner\n";
    assert_eq!(viewport_shows_settled_interrupt(screen), false);
}

#[test]
fn pause_hook_wait_real_cli_suffix_placeholder_and_background_terminal_is_settled() {
    let screen = "\
bash\n\
$\n\
sleep 30\n\
■ Conversation interrupted - use /feedback if something went wrong\n\
› Ask Codex to do anything";
    assert_eq!(viewport_shows_settled_interrupt(screen), true);

    let follow_up = "\
■ Conversation interrupted - use /feedback if something went wrong\n\
› Ask a follow-up question";
    assert_eq!(viewport_shows_settled_interrupt(follow_up), true);
}

#[test]
fn pause_hook_wait_unknown_suffix_typed_text_shell_and_footer_are_not_settled() {
    assert_eq!(
        viewport_shows_settled_interrupt(
            "■ Conversation interrupted - the agent kept going\n\
             › Ask Codex to do anything"
        ),
        false
    );
    assert_eq!(
        viewport_shows_settled_interrupt(
            "■ Conversation interrupted - use /feedback if something went wrong\n\
             › Ask Codex to do anything else"
        ),
        false
    );
    assert_eq!(
        viewport_shows_settled_interrupt(
            "■ Conversation interrupted - use /feedback if something went wrong\n\
             bash\n\
             $\n\
             sleep 30\n"
        ),
        false
    );
    assert_eq!(
        viewport_shows_settled_interrupt(
            "■ Conversation interrupted - use /feedback if something went wrong\n\
             › count from 1 to 25\n"
        ),
        false
    );
    assert_eq!(
        viewport_shows_settled_interrupt(
            "■ Conversation interrupted - use /feedback if something went wrong\n\
             › Ask Codex to do anything\n\
             Working (2s • esc to interrupt)"
        ),
        false
    );
}
