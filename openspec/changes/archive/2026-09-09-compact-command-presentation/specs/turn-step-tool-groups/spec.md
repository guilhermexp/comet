## MODIFIED Requirements

### Requirement: Compact consistent event typography
The transcript SHALL use compact event rows and regular sans typography consistent with narrative text. Command headers SHALL use Ran command or Running command and a bounded summary in a quieter tone. Full invocation and output SHALL remain monospaced in the expanded payload. Failure colors SHALL remain semantic. Expanded command headers and payloads SHALL share a single frame. Reasoning bodies SHALL have an inset left rule distinct from narrative.

#### Scenario: Mixed narrative and tool activity
Test: unit — command projection and full invocation retention; none — native GPUI visual acceptance.
- **WHEN** a turn contains long or compound commands and open reasoning
- **THEN** command headers show bounded sans summaries, with action stronger than detail
- **AND** expanding a command reveals its retained invocation and output inside the header frame
- **AND** reasoning has a left rule, respects explicit disclosure choices and shows the spiral only while active
