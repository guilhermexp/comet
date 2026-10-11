## ADDED Requirements

### Requirement: Worked Project rows show current checkout diff totals
Expanded Projects worked rows SHALL show their own checkout additions and deletions as +A / −D, using the Project row colors and separator. Names SHALL truncate before counters, and clicks SHALL continue to reveal the same path.

#### Scenario: Exact checkout matching
Test: unit — GPUI TestAppContext layout and mock RPC.
- **WHEN** the watch reports a changed checkout matching a Worked Project path
- **THEN** its row shows that checkout's additions and deletions at the right
- **AND** unrelated checkout diffs never substitute for missing diffs

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
- **AND** the stream restarts only when its path set or local device changes
- **AND** collapsing, hiding or emptying the section releases its stream

### Requirement: Checkout diff watch pins multiple optional checkouts
WatchCheckoutDiffs SHALL accept optional cwds in addition to the legacy cwd, deduplicate paths, and retain every successfully pinned checkout until the single stream is dropped. CheckoutDiff SHALL retain its existing shape.

#### Scenario: Multiple pins with one unavailable checkout
Test: unit — engine RPC with temporary Git checkouts and short orphan grace.
- **WHEN** cwd and cwds identify two orphan Git checkouts, duplicates and a non-Git folder
- **THEN** one stream captures both Git checkouts despite the folder failure
- **AND** reconcile retains both checkouts while the stream lives
- **AND** dropping the stream releases every pin and reconcile retires the orphan diffs after grace
