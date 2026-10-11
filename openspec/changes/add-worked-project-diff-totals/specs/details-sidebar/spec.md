## ADDED Requirements

### Requirement: Worked Project rows show current checkout diff totals
Expanded Projects worked rows SHALL show their own checkout additions and deletions as +A / −D, using the Project row colors and separator. Names SHALL truncate before counters, and clicks SHALL continue to reveal the same path.

#### Scenario: Exact checkout matching
Test: unit — GPUI TestAppContext layout and mock RPC.
- **WHEN** the watch reports a changed checkout matching a Worked Project path
- **THEN** its row shows that checkout's additions and deletions at the right
- **AND** unrelated checkout diffs never substitute for missing diffs

#### Scenario: Monorepo subfolder or noncanonical registered path
Test: none — existing unit fixtures cover exact cwd matching and unrelated checkouts, but do not cover a registered monorepo subfolder or symlink with a canonical Git root diff.
- **WHEN** a Worked Project is registered below the repository root (a monorepo subfolder) or through a noncanonical path such as a symlink
- **AND** the engine reports its checkout diff with the canonical Git root as cwd
- **THEN** the row shows no counter because its registered path does not exactly match that cwd
- **AND** checkout totals are not attributed to the subfolder or symlink through a fallback

#### Scenario: Worktree, clean and unavailable checkouts
Test: unit — GPUI TestAppContext with checkout frames.
- **WHEN** a row represents a Worker worktree
- **THEN** totals use its own path
- **AND** clean, non-Git and unavailable checkouts show no counter

#### Scenario: Live local watch lifecycle
Test: unit — mock RPC requests and incremental frames.
- **WHEN** an expanded nonempty section is visible in Orchestrator mode
- **THEN** one local WatchCheckoutDiffs stream pins its set of paths, independently of the Chat host
- **AND** changes update totals without polling
- **AND** the active stream restarts only when its path set or local device changes
- **AND** collapsing, hiding or emptying the section, or hiding the Details panel releases its stream
- **AND** showing Details again subscribes for the visible paths

#### Scenario: Subscription failure and disconnection
Test: unit — lifecycle mock RPC with closed outbound channel, completed stream and connection EOF.
- **WHEN** subscription fails or its stream ends
- **THEN** the matching watch key and stale totals are cleared, allowing a subsequent render to subscribe again
- **AND** cleanup notifies only if totals were visible, so immediate failures without totals do not drive a redraw loop
- **AND** an obsolete task cannot clear another watch key

### Requirement: Checkout diff watch pins multiple optional checkouts
WatchCheckoutDiffs SHALL accept optional cwds in addition to the legacy cwd, deduplicate paths, and retain every successfully pinned checkout until the single stream is dropped. CheckoutDiff SHALL retain its existing shape.

#### Scenario: Multiple pins with one unavailable checkout
Test: unit — engine RPC with temporary Git checkouts and short orphan grace.
- **WHEN** cwd and cwds identify two orphan Git checkouts, duplicates and a non-Git folder
- **THEN** one stream captures both Git checkouts despite the folder failure
- **AND** reconcile retains both checkouts while the stream lives
- **AND** dropping the stream releases every pin and reconcile retires the orphan diffs after grace
