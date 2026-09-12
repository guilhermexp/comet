# worker-terminal-initial-presentation Specification

## Purpose
Present an existing CLI Worker's first terminal view as a completed screen while retaining its available history and live output continuity.

## Requirements

### Requirement: First terminal presentation is complete

The application SHALL keep historical intermediate screens out of the renderer until it has consumed the output accumulated at opening. Elapsed time and history size MUST NOT authorize presenting an incomplete historical screen. History acquisition SHALL start after viewport geometry is available and SHALL drain without delays intended for idle live observation.

#### Scenario: Long history on first opening
Test: unit — workers terminal replay and grid projection; none — native GPUI visual validation.
- **WHEN** an unvisited Worker has accumulated multiple output chunks
- **THEN** the first published terminal grid shows the recovered end of that history, with no intermediate scrolling frames
- **AND** the retained history remains scrollable after loading

#### Scenario: Worker continues producing output
Test: unit — opening output boundary and incremental terminal state.
- **WHEN** a Worker keeps producing output throughout initial recovery
- **THEN** recovery completes at the recorded opening boundary without waiting for the Worker to become idle
- **AND** subsequent bytes continue from the consumed position without skipping or duplication

#### Scenario: Initial read fails
Test: unit — replay state on transport failure; none — native error presentation.
- **WHEN** a historical read fails before first recovery completes
- **THEN** the error is visible and the partial terminal grid remains unpublished
- **AND** successful retries can complete the same recovery

#### Scenario: Reopen a loaded terminal
Test: unit — retained terminal state and scroll position.
- **WHEN** the user leaves and reopens a fully loaded Worker terminal
- **THEN** its existing grid and intentional scroll position remain available without repeating initial loading
