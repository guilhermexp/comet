## ADDED Requirements

### Requirement: A partially painted selector is not an answerable menu

`viewport_has_menu_prompt` SHALL treat a footer that opens with a passive
selector prefix as passive unless the same footer also carries an interactive
qualifier, so a scan landing mid-repaint does not raise attention.

#### Scenario: The partial repaint stays quiet

Test: `ignores_claude_subagent_footer_during_partial_repaint`

- **WHEN** the visible footer reads `↑/↓ to select · Enter to`
- **THEN** no menu prompt is reported

#### Scenario: A real menu with the same prefix still answers

Test: `detects_qualified_menu_with_same_arrow_select_prefix`

- **WHEN** the footer carries that prefix together with a confirm or cancel action
- **THEN** the menu prompt is reported

### Requirement: A transcript shows what the user typed

Transcript sanitization SHALL unwrap a `<user_query>` wrapper to its content and
SHALL drop `<system-reminder>` and `<user_info>` blocks, so harness injections
never appear as user turns.

#### Scenario: Injections are dropped and the query is unwrapped

Test: `grok_transcript_entries_skip_injections_and_unwrap_user_query`

- **WHEN** a provider transcript carries `<user_info>`, `<system-reminder>` and
  a `<user_query>` wrapper
- **THEN** the only user entry is the text inside `<user_query>`
- **AND** no entry carries the injected blocks

### Requirement: The project projection carries the checkout's branch

The `comet-local` project projection SHALL emit `gitBranch` for a project whose
checkout has a readable HEAD, resolved by reading `.git` (following a
worktree's `gitdir:` file) rather than by spawning git.

#### Scenario: Checkout, worktree and detached HEAD all resolve

Test: `git_head_branch_reads_checkout_worktree_and_detached_head`

- **WHEN** the checkout is a plain repository, a linked worktree, or on a
  detached HEAD
- **THEN** the branch name, the worktree's branch, or a short sha is returned
- **AND** a directory that is not a checkout returns nothing

### Requirement: A hook reaches the Host without the user's proxy

Every hook script that POSTs a lifecycle event to loopback SHALL opt out of the
environment's proxy configuration, so a configured `HTTP_PROXY`/`ALL_PROXY`
cannot swallow the event.

#### Scenario: No hook script posts to loopback through a proxy

Test: `all_hook_scripts_bypass_the_proxy_for_the_loopback_post`

- **WHEN** a hook script contains a curl call posting to `127.0.0.1`
- **THEN** that call passes `--noproxy`
- **AND** this holds for every runtime's hook script, not only the shared one
