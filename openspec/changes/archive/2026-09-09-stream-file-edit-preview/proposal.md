## Why
File previews arrive in large byte batches and display tall static diffs while input is being generated. The requested compact typing effect needs timed progressive previews and bottom-aligned live content.

## What Changes
- Refresh partial Write/Edit input at 100ms cadence after initial semantic preview, while retaining bounded incremental parsing and final flush.
- Show a 72px collapsed window with the latest 15 generated lines, bottom aligned after three lines; wrap long text without horizontal scrolling.
- Skip syntax highlighting while active. After completion request highlighting after 50ms, show stats and allow header/body expansion up to 200px with vertical scrolling.
- Add filename shimmer and preserve existing spinner, diff colors, rounded card surfaces, errors and stable tool identities.

## Impact
Harness partial input decoder, native UI file card and offline mock fixture. No wire/schema changes.
