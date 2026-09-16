## Context
TaskSnapshot already renders the requested item row when open; its fold fallback currently selects false.

## Decisions
Use true as the existing optional fold's fallback. Preserve explicit fold values and all item rendering. This applies to streaming and retained task snapshots so completion does not hide the task.

## Risks
More vertical space is used by default. The existing disclosure still allows collapsing the list.
