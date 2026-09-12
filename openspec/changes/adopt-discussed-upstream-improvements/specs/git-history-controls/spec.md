## Purpose

Allow users to navigate and search Git history using the existing source-control panel and bounded process execution.

## ADDED Requirements

### Requirement: Search and branch-tip navigation
Git history SHALL support searching commits, navigating the graph and switching to branch-tip views while keeping pagination and selected-commit details coherent.

#### Scenario: Find and inspect a commit
Test: unit + integration — ProcessRunner fixtures and history projection; none — BCU.
- **WHEN** a user searches history and selects a matching commit or branch tip
- **THEN** the selected commit and graph correspond to that query and repository

#### Scenario: Select a result after leaving the search input
Test: unit — history search blur regression; none — BCU native click.
- **WHEN** clicking a matching commit moves focus out of the search input
- **THEN** the query and filtered rows remain stable and the clicked commit opens

### Requirement: Configurable history columns
Git history SHALL allow users to choose visible columns and preserve that preference without changing commit identity or navigation.

#### Scenario: Change displayed columns
Test: unit — settings and columns; none — native layout.
- **WHEN** a user changes the history column selection
- **THEN** the selected columns update while the current commit remains selected
