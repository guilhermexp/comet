# chat-transcript-export Specification

## Purpose

Export Chat Transcripts into Markdown, JSON, or Plain Text files and copy them to clipboard with sanitized projection.

## Requirements

### Requirement: A Chat can be exported in three formats

The chat context menu SHALL offer six actions — download and copy, each as
Markdown, JSON and Text — for the Chat the menu was opened on, whether or not
that Chat is the selected one.

#### Scenario: Export a Chat that is not selected
Test: unit — transcript resolution by chat id (selected vs. non-selected path).

- **WHEN** the user opens the context menu on a Chat other than the selected one and picks any export action
- **THEN** the exported content is that Chat's transcript, not the selected Chat's

#### Scenario: Download writes to the Downloads directory
Test: unit — filename builder; visual — the file appears and the notice names it.

- **WHEN** the user picks a download action
- **THEN** a file is written to the user's Downloads directory named after the Chat title and the first eight characters of its id
- **AND** the sidebar notice names the file that was written
- **AND** no save dialog is shown

#### Scenario: Copy writes to the clipboard
Test: visual — paste elsewhere after the action.

- **WHEN** the user picks a copy action
- **THEN** the rendered content is on the clipboard
- **AND** the sidebar notice confirms the copy

#### Scenario: A failed export says why
Test: unit — the outcome-message decision.

- **WHEN** an export cannot be written or copied
- **THEN** the sidebar notice states what failed
- **AND** no file is left behind

### Requirement: An export carries only what the Chat Transcript carries

An export SHALL be rendered from the Chat Transcript and SHALL NOT read the Run
Journal or resolve sidecar output blobs. A tool part whose input the transcript
stripped SHALL export without that input rather than recovering it.

#### Scenario: A tool with a stripped input
Test: unit — renderer over a transcript entry whose tool input was sanitized.

- **WHEN** a Chat contains a tool part whose input the transcript does not carry
- **THEN** the export names the tool and does not name the input
- **AND** no journal file is read

#### Scenario: A tool with a heavy output
Test: unit — renderer over an entry carrying `outputBytes` and an output ref.

- **WHEN** a Chat contains a tool part whose full output lives in a sidecar
- **THEN** the export records the tool and the output's size
- **AND** the sidecar is not fetched

### Requirement: The three formats agree about the Chat

The Markdown, JSON and Text renderings of one Chat SHALL be derived from a
single intermediate document, so that all three report the same messages and
the same artifacts.

#### Scenario: The same Chat rendered three ways
Test: unit — one transcript rendered in all three formats, compared.

- **WHEN** one Chat is rendered as Markdown, JSON and Text
- **THEN** the three name the same set of artifacts
- **AND** the three cover the same messages in the same order

#### Scenario: Tool parts render as one line
Test: unit — renderer over Bash, Write, Edit, Read and an unrecognized tool.

- **WHEN** a message contains tool parts
- **THEN** each renders as a single entry shaped by its tool
- **AND** no tool's full output appears in any format

#### Scenario: JSON serializes only the shared export projection
Test: unit — JSON over tool output/refs plus skipped transcript-only parts.

- **WHEN** a Chat Transcript tool part carries inline output, diff or sidecar references
- **THEN** JSON contains only the same projected text/tool sequence used by Markdown and Text
- **AND** JSON contains no output, diff, outputRef or diffRef field
- **AND** reasoning, input and workflow parts are absent from every format

### Requirement: An export opens with what the Chat produced

An export SHALL begin with an artifact index listing the files the Chat wrote,
the subagents it ran, and the outputs too heavy to carry inline, each addressed
by its position in the export.

#### Scenario: A Chat that wrote files and ran a subagent
Test: unit — artifact index over a transcript containing writes and a subagent chip.

- **WHEN** a Chat containing file writes and a subagent run is exported
- **THEN** the artifact index names each written file and records that the subagent ran
- **AND** each entry gives the message and part it came from

#### Scenario: A Chat that produced nothing indexable
Test: unit — artifact index over a text-only transcript.

- **WHEN** a Chat with no writes, subagents or heavy outputs is exported
- **THEN** the export states that there are no artifacts rather than omitting the section

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

#### Scenario: The worker join fails while the file is still delivered
Test: unit — outcome folding for a failed workers join.

- **WHEN** the CLI workers cannot be resolved (worker link state is unreadable) and the export is still delivered
- **THEN** the sidebar notice reports the delivery as incomplete, naming the destination and the reason, in the failure tone
- **AND** a delivery that itself failed keeps its own failure reason
