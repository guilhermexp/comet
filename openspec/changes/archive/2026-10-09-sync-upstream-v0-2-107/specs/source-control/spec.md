## ADDED Requirements

### Requirement: Turn snapshots preserve checkout storage boundaries
Automatic turn snapshots SHALL keep their private objects outside the checkout object database, bound untracked capture by size/count and deadline, and remove obsolete private stores. Partial captures SHALL keep layered turn diffs stable without changing the real index or tracked files.

#### Scenario: An untracked build directory exceeds capture limits
Test: unit + integration — turn snapshots and diff-sync churn fixtures.
- **WHEN** a turn snapshot encounters oversized untracked output
- **THEN** bounded capture excludes oversized units without writing their objects into the checkout database
- **AND** tracked changes and smaller included units remain observable

#### Scenario: A snapshot is replaced or its capture times out
Test: integration — snapshot lifecycle and cancellation fixtures.
- **WHEN** a turn snapshot becomes obsolete or capture exceeds its deadline
- **THEN** owned capture children terminate and obsolete private storage is reclaimed
