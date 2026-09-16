## ADDED Requirements

### Requirement: Partial file previews follow the doc commit cadence
The interval that gates partial file-tool previews SHALL be the host's streamed
commit interval, taken from the shared constant rather than restated in the
harness. The gate SHALL remain in place so the number of preview events per
streamed body stays bounded, and the existing first-emission and
first-semantic-follow-up exemptions SHALL be preserved.

#### Scenario: A large body streams in many deltas
Test: unit — gate interval against the shared constant; existing linearity suites.
- **WHEN** a Write or Edit streams a body across many deltas
- **THEN** previews are emitted no faster than one commit window apart
- **AND** the emitted event count per streamed megabyte stays within its existing bound

#### Scenario: The commit cadence is retuned
Test: unit — gate interval against the shared constant.
- **WHEN** the streamed commit interval changes
- **THEN** the preview gate changes with it, with no second literal to update
