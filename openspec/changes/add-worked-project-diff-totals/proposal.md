# Change: Worked Project diff totals

## Why
Projects worked identifies touched checkouts but does not expose their current uncommitted additions and deletions.

## What Changes
- Show exact-path working-tree +A / −D totals at the right of each expanded Worked Project row; preserve reveal and truncation.
- Subscribe locally once for the sorted set of visible paths, including Worker worktrees; no fallback, polling or hidden-section subscription.
- Extend WatchCheckoutDiffs with optional cwds while preserving cwd and CheckoutDiff.

## Impact
crates/ui/src/details_sidebar and crates/engine/src/rpc.rs. No persisted or protocol entity shape changes.
