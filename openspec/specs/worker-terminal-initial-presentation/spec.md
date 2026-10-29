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

### Requirement: Attach consumes available history without waiting for control operations

Terminal recovery SHALL consume the retained journal from its earliest available byte in bounded incremental reads. It MUST NOT discard additional retained output because an old cursor predates retention. Opening SHALL use a fixed journal boundary and MUST NOT wait for resize or input-mode snapshots before recovering the screen.

#### Scenario: Retained journal larger than a transport page
Test: integration — workers-unpeel terminal reader; unit — Workers terminal consumer.
- **WHEN** an initial cursor predates the journal retention floor and multiple pages remain
- **THEN** recovery consumes every retained byte in order with bounded reads
- **AND** subsequent live bytes continue without duplication or skipped pages
- **AND** the UI reports that earlier terminal output is no longer retained

#### Scenario: Host control socket is slow or absent
Test: integration — isolated control socket; unit — Workers replay/resize gating.
- **WHEN** the journal is readable but viewport or resize cannot respond promptly
- **THEN** initial recovery completes from the journal independently
- **AND** input-mode synchronization does not discard successfully read output

#### Scenario: Journal not yet created
Test: integration — private delayed journal.
- **WHEN** attach precedes creation of the selected Worker's journal
- **THEN** it treats absence as an empty pending stream and consumes its first bytes on a later read

#### Scenario: Empty retained gap with an available Host screen
Test: unit — matching and stale Host snapshot recovery.
- **WHEN** retention rebases the cursor with no retained bytes in that read
- **THEN** a matching Host viewport can restore the current screen asynchronously
- **AND** a late or newer snapshot cannot overwrite or duplicate journal bytes

### Requirement: History navigation preserves the reader and live TUI interaction

Stopped Worker scrolling SHALL navigate the local terminal history without writing to its PTY. Live Workers SHALL retain normal mouse and alternate-screen interaction and provide Shift+wheel to navigate local history. New output MUST NOT force an intentionally scrolled viewport to the tail. Reclaimed bytes and overwritten alternate-screen frames MUST NOT be presented as recoverable conversation history.

#### Scenario: Stopped Worker retains mouse modes
Test: unit — Workers terminal scroll consumer.
- **WHEN** an exited Worker retains mouse reporting or alternate-scroll modes
- **THEN** wheel navigation scrolls the available local history and emits no input

#### Scenario: Read earlier output while the Worker runs
Test: unit — Workers terminal feed/scroll consumer; none — isolated native fixture appearance.
- **WHEN** the user uses Shift+wheel to reach the beginning of available local history and new output arrives
- **THEN** the earlier text remains visible without a forced jump to the tail
- **AND** ordinary wheel input at the live screen continues to use the TUI's requested protocol
