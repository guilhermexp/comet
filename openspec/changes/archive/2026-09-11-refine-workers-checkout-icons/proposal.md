## Why
The added Local/main subtitle misrepresents the project container and adds unwanted noise. The user clarified that worktrees belong inside the project tree and use a worktree glyph in place of a folder.

## What Changes
Remove checkout subtitles. Keep base project folders, local Workers directly beneath them, and named nested worktree rows with the existing worktree icon. Confirmed PRs use an icon with tooltip.

## Capabilities
### Modified Capabilities
- `workers-sidebar-context`: compact project/worktree hierarchy.

## Impact
Workers sidebar rendering and an icon-only variant of the shared PR badge. No registry or session changes.
