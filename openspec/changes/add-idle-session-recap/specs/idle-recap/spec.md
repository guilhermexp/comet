# Specification: Idle Session Recap

## ADDED Requirements

### Requirement: Transcript Selection
The engine SHALL project the stored session transcript into a bounded, chronological transcript tail.
Tool calls, tool outputs, system messages, and empty messages SHALL be excluded.
The projection SHALL walk from the newest messages backwards, bounded by a maximum of 40 messages, a total budget of 6,000 characters, and up to 800 characters per message sliced at a word boundary.
Selected messages SHALL be returned in chronological order.

#### Scenario: Long conversation tail
- **GIVEN** a conversation with 100 turns
- **WHEN** `select_recap_transcript` is executed
- **THEN** it SHALL include the most recent messages up to the character budget and exclude the initial messages.

### Requirement: Recap Prompt Construction
The recap prompt SHALL instruct the model to produce a summary of under 40 words, in 1-2 plain sentences with no markdown formatting.
The prompt SHALL instruct the model to write in the exact same language the conversation uses.
When an overall goal or title exists for the chat, it SHALL be included as an anchor.

#### Scenario: Goal anchor included
- **GIVEN** a chat titled "Fix memory leak in buffer"
- **WHEN** `build_recap_prompt` is called with this goal
- **THEN** the prompt SHALL contain "Overall goal: Fix memory leak in buffer".

### Requirement: Validation and Cleaning
The validator SHALL strip code blocks, markdown punctuation, bullet points, and surrounding quotes.
The validator SHALL strip conversational preambles (such as "Sure:", "Here is the recap:").
The validator SHALL clamp the text at a word boundary to at most 280 characters.
If no clean content remains, it SHALL return `None`.

#### Scenario: Preamble stripping
- **GIVEN** a raw completion "Sure: Recap: Fixed the leak; next is running the test suite."
- **WHEN** `validate_recap` is executed
- **THEN** it SHALL return "Fixed the leak; next is running the test suite.".

### Requirement: Idle Arming Policy
The UI recap evaluator SHALL decide whether to arm, keep, clear, or ignore based on session idle state:
- If disabled or cannot generate, it SHALL clear any existing entry.
- If streaming or compacting, it SHALL clear any existing entry.
- If the entry's epoch matches the current message count, it SHALL keep the existing entry without re-arming.
- If message count is zero, it SHALL preserve an existing entry to prevent startup hydration races.
- If composer has draft text, it SHALL clear any stale entry.
- Otherwise, it SHALL arm a timer with delay clamped between 60 and 600 seconds.

#### Scenario: Epoch matching keeps entry
- **GIVEN** an existing recap generated at epoch 12
- **AND** the current message count is 12
- **WHEN** `evaluate_idle_recap` evaluates the state
- **THEN** it SHALL return `IdleRecapAction::Keep`.

### Requirement: Retention and Pruning
Persisted recaps SHALL be pruned on read and write.
Entries older than 24 hours (`IDLE_RECAP_MAX_AGE_MS`) SHALL be dropped.
Entries with timestamps in the future SHALL be dropped.
The map SHALL be capped at the 50 newest entries.

#### Scenario: Expired entry dropped
- **GIVEN** a recap entry generated 25 hours ago
- **WHEN** `prune_idle_recaps` runs
- **THEN** the entry SHALL be removed from the map.

### Requirement: Isolated recap execution
Recaps SHALL reuse the tool-free isolated execution used by automatic titles, with recap-specific instructions and the original recap prompt. The process SHALL run in a temporary directory with autoapproval and Workers MCP disabled. An unsupported harness SHALL use an enabled compatible harness or return no recap. Tool events, incomplete streams and timeout SHALL never yield a successful recap.

#### Scenario: Recap stays outside the project
- **WHEN** a recap is generated for a project Chat
- **THEN** the isolated harness receives the recap prompt unchanged and a temporary cwd
- **AND** the cleaned recap is returned without creating a Chat turn
- **Test:** unit — `recap_uses_isolated_execution_and_preserves_prompt`

#### Scenario: Native isolated harness restrictions
- **WHEN** Claude or Codex executes a recap
- **THEN** it receives recap instructions with the same tool and permission restrictions as title generation
- **Test:** integration — `isolated_recap_disables_tools_and_denies_unexpected_permissions`, `isolated_recap_preserves_read_only_and_replaces_coding_instructions`
