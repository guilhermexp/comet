## Why
Workers repeat ambiguous branch glyphs, hide branch names, gate PRs on worktree registration, and retain empty projects through automatic selection.

## What Changes
- Show checkout type and current branch once per project; keep runtime and activity on session rows.
- Resolve PRs for relevant local checkouts as well as worktrees.
- Remove automatic empty-project selection and clear selection when its last session disappears; preserve explicit launch targets and the project ledger.

## Capabilities
### New Capabilities
- `workers-sidebar-context`: consistent checkout context and working-set visibility.

## Impact
Workers frontend presentation/model, shared WorkersProject branch resolution, existing PR subscription inputs. No customer/session deletion or schema migration.
