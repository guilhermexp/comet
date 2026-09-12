## Why
Native tracing shows the sticky header switching between current and previous user rows on each stream chunk. The scrollbar offset may still be the past-end sentinel before layout; user geometry records a post-layout top against this pre-layout offset. Command disclosures also retain a closing body for a removed animation and wait for an unrelated repaint to finish closing.
## What Changes
Publish the sticky scroll offset only after list layout completes, normalize it to the valid viewport range, and record user geometry against the offset from the same completed layout. Close command/tool details directly when their open state changes, without retaining a body for a nonexistent animation.
## Impact
Transcript sticky projection and tool detail disclosure only.
