## ADDED Requirements

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
