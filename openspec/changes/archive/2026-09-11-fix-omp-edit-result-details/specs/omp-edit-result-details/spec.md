## Purpose

Preserve authoritative OMP edit metadata so the Chat Transcript can identify the edited file and display the available diff and line counts.

## ADDED Requirements

### Requirement: Preserve authoritative single-file edit metadata
The OMP adapter SHALL normalize file snapshots from `result.details` into the existing ToolDiff with the same tool ID, preserving the legacy top-level format. It SHALL NOT fabricate missing content or select one file from a multi-file result as the whole edit.

#### Scenario: Nested snapshots reach the file card pipeline
Test: unit — `crates/harness/src/omp/normalize.rs`.
- **WHEN** an edit result contains a path, oldText and newText inside details
- **THEN** its ToolResult carries that path and both snapshots, allowing the document to recover the filename, diff preview and statistics

#### Scenario: Legacy and empty replacement remain supported
Test: unit — `crates/harness/src/omp/normalize.rs`.
- **WHEN** snapshots are top-level or the new content is an empty string
- **THEN** the adapter retains the valid diff including the empty replacement

#### Scenario: Missing snapshots and multiple files are not invented
Test: unit — `crates/harness/src/omp/normalize.rs`.
- **WHEN** the result lacks snapshots or contains multiple per-file results
- **THEN** no single-file diff is synthesized from incomplete or unrelated contents
