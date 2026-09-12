## ADDED Requirements
### Requirement: Sticky geometry uses a consistent layout coordinate system
The sticky turn SHALL use a scroll offset within the laid-out viewport range. Recorded user positions and scroll offsets SHALL come from the same completed layout. Pending end anchors during streaming SHALL NOT switch the header to another turn.
#### Scenario: Streaming while anchored at the end
Test: unit — offset projection; none — native multi-turn streaming.
- **WHEN** a stream chunk sets a past-end list anchor before layout
- **THEN** the sticky projection uses the valid scroll range and retains the current turn
### Requirement: Tool details close without waiting for another event
A tool detail using natural wrapped height SHALL unmount its body immediately when closed and SHALL NOT retain it for an animation that is not running.
#### Scenario: Close a completed tool detail
Test: none — native GPUI interaction.
- **WHEN** the user closes an expanded tool detail after streaming has stopped
- **THEN** the next rendered frame shows the closed detail without requiring a timer, pointer movement or a new stream event
