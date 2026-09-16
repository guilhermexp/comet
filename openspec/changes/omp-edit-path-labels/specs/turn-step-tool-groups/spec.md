## ADDED Requirements

### Requirement: Edit cards identify recorded targets
OMP hashline edits SHALL derive a unique file path from canonical tagged sections when an explicit path is absent. Multiple distinct paths SHALL NOT be represented as one file. Legacy cards MAY recover a unique path from recorded result headers; absent targets SHALL display a non-clickable unavailable label instead of an empty label.

#### Scenario: Hashline edit target
Test: unit — proto parser and OMP normalization.
- **WHEN** an edit contains one unique tagged file section
- **THEN** its normalized call identifies that file

#### Scenario: Multiple or absent targets
Test: unit — proto parser and OMP normalization.
- **WHEN** tagged sections identify multiple files or no valid target
- **THEN** no arbitrary file path is invented

#### Scenario: Legacy card presentation
Test: none — native visual review.
- **WHEN** an old edit card lacks a path
- **THEN** it uses a unique recorded result target or a visible unavailable label
