## ADDED Requirements

### Requirement: Readable Markdown table columns
Markdown tables SHALL account for inline chip padding, icon width and text scale when measuring columns. Short words SHALL remain intact where their measured width fits the bounded column minimum. Wide tables SHALL overflow within a horizontally scrollable viewport constrained to the transcript width.

#### Scenario: Verdict and code columns
Test: unit — column geometry and Markdown measurement.
- **WHEN** a table mixes short verdicts, prose and inline code
- **THEN** column minimums preserve short verdicts and include inline box decoration

#### Scenario: Narrow transcript
Test: none — native visual review.
- **WHEN** the sum of column minimums exceeds the transcript width
- **THEN** the table scrolls horizontally within that width
