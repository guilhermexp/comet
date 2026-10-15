# Spec Delta

## Purpose

Worker task state, interruption and transcript reads report what the Worker actually did.

## ADDED Requirements

### Requirement: A pause for background work is not task completion

The system SHALL NOT announce or match `completed` for a Worker episode while a background task launched by the Worker's current runtime has no finish notification in the provider transcript, unless the Worker's output has been silent for at least 15 minutes.

#### Scenario: Stop while background shells run
- Test: unit — `background_tasks` scanner; integration — `tests/parent_notifications.rs` `stop_with_background_tasks_pending_is_not_completion`.

- **GIVEN** a bound Worker episode with a `Stop` and quiescent output
- **WHEN** a background shell or async agent launched by that runtime has not reported back
- **THEN** no `completed` notification is pending and `wait_for_status(completed)` does not match
- **AND** the `Stop` after the task reports back completes the episode

### Requirement: An Escape interrupt settles the running turn

The session Host SHALL record the time of every bare Escape written to the PTY. While that time is newer than the last lifecycle hook, the system SHALL report the Worker turn as settled (not `working`) without confirming a Stop. A `completed` wait SHALL end with `interrupted: true`.

#### Scenario: User interrupts a just-submitted prompt
- Test: unit — `session_host` `only_a_bare_escape_counts_as_an_interrupt_key`, `activity_bridge` `escape_after_the_last_hook_settles_the_turn_until_the_next_hook`.

- **GIVEN** a Worker whose last hook is `UserPromptSubmit`
- **WHEN** a bare Escape is written to its PTY
- **THEN** its activity is no longer `working` and no completion is announced
- **AND** the next hook rearms normal lifecycle tracking

### Requirement: Transcript reads return whole messages and can reach all content

The shared transcript reader SHALL NOT truncate message text. "Whole transcript" SHALL read at least 64 MiB of provider history and SHALL mark anything older as omitted. `read_transcript` SHALL default to the saved transcript range, accept `0` as whole, and return pages of at most 96 KiB with offsets that reach every byte.

#### Scenario: Long report in a large session
- Test: unit — `transcripts` `long_messages_are_kept_whole`, `whole_markdown_window_reads_past_the_stream_cap`; `controller_mcp` `pages_walk_the_whole_transcript_without_losing_bytes`; live read of session `5723366d…`.

- **GIVEN** a 3.5 MB Claude transcript whose final message has 20 425 characters
- **WHEN** the Orchestrator reads it with the saved "Whole transcript" range
- **THEN** that message is returned whole and the first user prompt is present
- **AND** a response larger than one page reports `truncated` with an offset to continue

### Requirement: Comet's tool failures stay out of Worker prompts

The controller tool description SHALL instruct the Orchestrator not to ask a Worker to work around Comet read, wait or notification failures.

#### Scenario: Orchestrator reads a long report
- Test: manual — controller tool description and `help` reviewed.

- **WHEN** a report exceeds one page
- **THEN** the documented path is paging `read_transcript`, not a new request to the Worker
