## ADDED Requirements

### Requirement: Markdown in the input is decorated without mutating text

The composer input SHALL visually style markdown constructs (headings 1-6,
bold, italic, strikethrough, inline code, fenced code, quotes, list markers,
links) as paint-only text runs. The underlying text SHALL never change:
markers remain present (in faint ink), caret positions and mention chips are
unaffected, and inputs longer than 10 000 characters skip decoration.

#### Scenario: Typing styled constructs
Test: pure-scanner unit tests plus composer run-mapping tests.

- **WHEN** the input contains `## Title`, `**bold**`, `` `code` `` and a fenced block
- **THEN** each region paints with its style and its markers stay visible
- **AND** the serialized input text is byte-identical to what was typed

#### Scenario: Constructs that must not match
Test: unit — markdown_decor scanner tests (italic word-boundary, unclosed markers).

- **WHEN** the input contains `snake_case_name` or an unclosed `**marker`
- **THEN** no italic/bold styling is applied to them

#### Scenario: Oversized input
Test: unit — markdown_decor cap-skip test.

- **WHEN** the input exceeds 10 000 characters
- **THEN** decoration is skipped entirely and editing stays responsive

### Requirement: GitHub and YouTube URLs in sent messages render as chips

User-message text in the transcript SHALL render `github.com` URLs as a chip
labeled with the repository's `owner/repo` and `youtube.com`/`youtu.be` URLs
as a YouTube chip; clicking a chip opens the URL externally. Trailing
punctuation adjacent to a URL SHALL stay outside the chip, and all other URLs
render as plain text.

#### Scenario: GitHub link with trailing punctuation
Test: unit — url segmentation tests (trailing punctuation, owner/repo label).

- **WHEN** a sent message contains `see https://github.com/rust-lang/rust.`
- **THEN** a chip labeled `rust-lang/rust` renders and the final `.` stays as text

#### Scenario: YouTube short link
Test: unit — url segmentation tests (youtu.be host match).

- **WHEN** a sent message contains `https://youtu.be/abc123`
- **THEN** a YouTube chip renders and opens the URL on click

#### Scenario: Any other URL
Test: unit — url segmentation tests (non-matching hosts stay text).

- **WHEN** a sent message contains a non-GitHub, non-YouTube URL
- **THEN** it renders as plain text
