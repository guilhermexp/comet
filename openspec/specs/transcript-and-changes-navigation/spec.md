# transcript-and-changes-navigation Specification

## Purpose

Keep long agent output and large diffs readable through stable transcript projection, explicit copying, and contextual sticky navigation.

## Requirements

### Requirement: Transcript streaming preserves reading position and copy fidelity

The transcript SHALL anchor live output at the end only when following live content, restore the prior viewport when reopening, and copy visible message content faithfully.

#### Scenario: User reads older content during streaming

Test: deterministic viewport state tests plus headed transcript smoke.

- **WHEN** live output arrives while the user is reading above the follow threshold
- **THEN** the viewport does not jump to the end
- **AND** reopening the Chat restores the recorded reading position

#### Scenario: User copies one transcript entry

Test: transcript renderer unit test with text, Reasoning, tools, and attachments.

- **WHEN** the user invokes copy on a visible transcript entry
- **THEN** the clipboard receives that entry's human-readable visible content
- **AND** hidden credentials, raw tool payloads, and Run Journal-only fields are absent

### Requirement: Changes keeps the active file header visible

The Changes surface SHALL keep the active diff file header visible while its lines scroll and SHALL preserve theme/glass treatment without an extra shadow.

#### Scenario: Long diff crosses a file boundary

Test: deterministic diff viewport test plus headed Changes smoke.

- **WHEN** the active file's original header scrolls above the Changes viewport
- **THEN** an equivalent contextual header remains visible
- **AND** the next file header replaces it at the correct boundary

### Requirement: Thinking renders within operational context

Completed Thinking associated with tool execution SHALL render as Markdown inside the relevant tool-group or turn-step disclosure without hiding standalone Reasoning.

#### Scenario: Thinking precedes a tool group

Test: transcript projection and Markdown rendering unit tests.

- **WHEN** completed Thinking directly belongs to a grouped operational step
- **THEN** it appears inside that disclosure with Markdown formatting
- **AND** independent Reasoning continues to use the fork's first-class transcript part
