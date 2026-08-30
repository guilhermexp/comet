## ADDED Requirements

### Requirement: An export lists CLI workers associated with the Chat

An export SHALL include in its artifact index the CLI workers dispatched by the Chat,
recording each worker's session id and title across Markdown, Text, and JSON formats.

#### Scenario: A Chat that ran CLI workers
Test: unit — artifact index over a transcript with associated workers.

- **WHEN** a Chat with associated CLI workers is exported
- **THEN** the artifact index lists each worker with its session id and title in Markdown, Text, and JSON
- **AND** Markdown includes a worker count line in its header
- **AND** worker artifacts are ordered deterministically (active first, newer first)

#### Scenario: A Chat with no CLI workers produces baseline output
Test: unit — export comparisons with zero workers.

- **WHEN** a Chat with no associated CLI workers is exported
- **THEN** the Markdown, Text, and JSON outputs are byte-identical to the baseline export
- **AND** no worker count header is rendered in Markdown
