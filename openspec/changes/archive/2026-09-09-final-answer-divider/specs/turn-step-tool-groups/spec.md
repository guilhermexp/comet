## ADDED Requirements

### Requirement: Final answer is separated from completed activity
The transcript SHALL display one quiet horizontal rule across the content column between completed turn activity and its final answer. The rule SHALL remain below all activity when turn steps are expanded and below the summary when collapsed. It SHALL NOT introduce rules between intermediate commentary and tools or within the final answer.

#### Scenario: Toggle completed activity
Test: unit — existing completed turn projection; none — native GPUI separator appearance.
- **WHEN** a completed turn contains operational activity followed by a final answer
- **THEN** one separator appears after the activity and before the answer
- **AND** expanding or collapsing activity preserves that boundary without duplicating the separator
