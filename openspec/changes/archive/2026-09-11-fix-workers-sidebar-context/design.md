## Context
Project snapshots already carry current HEAD and worktree metadata. Session launch stamps may be stale. Empty selected projects bypass the working-set filter.

## Decisions
Use the project current branch for header, titlebar and PR lookup. Put readable checkout metadata under the project name instead of repeating branch glyphs on session rows. Query only working-set projects, keeping existing PR cache and settings gates. Selection may retain an empty project only from explicit project/launcher navigation, never from the first registry row or a removed session.

## Risks / Trade-offs
Missing branch metadata stays unknown rather than being called main. PR absence is not represented as proof that no PR exists. Collapsed parents and archive access retain existing behavior; no registry records or files are deleted.
