## RENAMED Requirements
- FROM: `### Requirement: Reasoning uses the reference spiral`
- TO: `### Requirement: Reasoning uses the working spinner`

## MODIFIED Requirements
### Requirement: Reasoning uses the working spinner
Active reasoning SHALL reuse the same native 3×3 gradient dot spinner as the working indicator, including its 2.5px cell size, colors and shared animation clock. It SHALL disappear completely when reasoning completes and SHALL NOT add a second spinner. Existing disclosure behavior SHALL remain unchanged.

#### Scenario: Thinking settles
Test: none — native visual acceptance of the existing shared spinner.
- **WHEN** active reasoning completes
- **THEN** the label becomes Thought and the spinner is removed, leaving no SVG/icon
- **AND** the row keeps its alignment and expandable content

### Requirement: Generic tools use their names directly
Generic Unknown tool headers without a specialized presenter SHALL display their icon and tool name without Ran tool or Running tool prefixes. The name SHALL retain truncation and the primary neutral header tone. Question tools SHALL use the interactive question history presenter instead. Eval headers SHALL display their title or tool name without Evaluated/Evaluating; MCP headers SHALL display server and tool without Called tool/Calling tool. Other specific semantic tool labels, failure presentation, active indicators and expanded payloads SHALL remain available.

#### Scenario: Generic tool settles
Test: unit — native stream copy projection.
- **WHEN** a generic tool without a specialized presenter is pending or completes
- **THEN** its normal header text is only its tool name
- **AND** eval and MCP headers omit execution verbs in both lifecycle states, while hub and command headers keep their semantic labels
- **AND** question tools render their question lifecycle and answers instead of an ask header

#### Scenario: Generic ask settles
Test: unit — native question projection.
- **WHEN** an ask tool is pending or completes
- **THEN** its specialized presenter displays Asking question, Waiting for response, or Answer/Answers as appropriate
- **AND** neither ask nor Ran tool is displayed as a generic header


### Requirement: Compact consistent event typography
The transcript SHALL use compact event rows and regular sans typography consistent with narrative text. Command headers SHALL use Ran command or Running command and a bounded summary in a quieter tone. Full invocation and output SHALL remain monospaced in the expanded payload. Failure colors SHALL remain semantic. Expanded command headers and payloads SHALL share a single frame. Reasoning bodies SHALL have an inset left rule distinct from narrative.

#### Scenario: Mixed narrative and tool activity
Test: unit — command projection and full invocation retention; none — native GPUI visual acceptance.
- **WHEN** a turn contains long or compound commands and open reasoning
- **THEN** command headers show bounded sans summaries, with action stronger than detail
- **AND** expanding a command reveals its retained invocation and output inside the header frame
- **AND** reasoning has a left rule, respects explicit disclosure choices and shows the working dot spinner only while active
