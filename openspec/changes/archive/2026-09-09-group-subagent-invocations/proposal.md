## Why
One task invocation fanning out multiple subagents currently renders repeated Agent rows.

## What Changes
Group siblings of the same recorded task invocation into a wrapping row with individual clickable pairs of a larger avatar followed by its own name and a shared lifecycle summary. Preserve separate docs, identities and ordinary/single-agent rows. Use the same doc-keyed avatar in the preview tab and the command-card neutral background for file edits.

## Impact
Native UI projection/render and mock fixture only. No wire changes.
