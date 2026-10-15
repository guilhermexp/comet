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
The UI recap evaluator SHALL decide whether to arm, keep, clear, or ignore based on session idle state. A stored recap SHALL stay visible until a newer recap replaces it; the only path that removes it is the feature being disabled.
- If disabled, it SHALL clear any existing entry.
- If the current epoch already failed to generate, it SHALL keep any existing entry and not arm.
- If streaming or compacting, it SHALL keep any existing entry and not arm.
- If the entry's epoch matches the current message count, it SHALL keep the existing entry without re-arming.
- If message count is zero, it SHALL preserve an existing entry to prevent startup hydration races.
- If composer has draft text, it SHALL keep any existing entry and not arm.
- Otherwise, it SHALL arm a timer with delay clamped between 60 and 600 seconds, leaving the existing entry visible until the new recap replaces it.

#### Scenario: Epoch matching keeps entry
- **GIVEN** an existing recap generated at epoch 12
- **AND** the current message count is 12
- **WHEN** `evaluate_idle_recap` evaluates the state
- **THEN** it SHALL return `IdleRecapAction::Keep`.

#### Scenario: A new turn does not erase the recap
- **GIVEN** an existing recap generated at epoch 12
- **WHEN** the chat starts streaming, or the composer holds a draft, or generation for the new epoch fails
- **THEN** `evaluate_idle_recap` SHALL return `IdleRecapAction::Keep`.

### Requirement: Retention and Pruning
Persisted recaps SHALL be pruned on read and write.
Recaps SHALL NOT expire by age.
Entries with empty text, a zero epoch, or a timestamp in the future SHALL be dropped.
The map SHALL be capped at the 50 newest entries.

#### Scenario: Old entry survives
- **GIVEN** a recap entry generated 25 hours ago
- **WHEN** `prune_idle_recaps` runs
- **THEN** the entry SHALL remain in the map.

### Requirement: Recap key ignores the working directory
A recap SHALL be stored under the chat's identity (kind, chat id and device), not under its working directory, so a chat whose cwd changes still finds its recap. Entries persisted under the older rooted key SHALL be migrated on load, keeping the newest one per chat.

#### Scenario: Rooted key migrated
- **GIVEN** a persisted entry keyed `orchestrator-chat:c1:device:d1:root:/work`
- **WHEN** the sidebar preferences load
- **THEN** the entry SHALL be found for the context key `orchestrator-chat:c1:device:d1:root:/other`.

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
