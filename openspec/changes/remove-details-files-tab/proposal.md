## Why
The app shows two file browsers side by side: the fork's own Files tab in the
Details sidebar (added 2026-08-20, before upstream shipped one) and the
upstream Files explorer (`crates/ui/src/files/`, Explorer | Changes). They
list the same checkout with different controls, icons and rules. The owner
keeps only the upstream explorer.

## What Changes
- **BREAKING** Remove the Details sidebar Files tab: the Details | Files pill
  strip, the file tree with its header controls (hidden toggle, new file/folder,
  search, collapse), inline create/rename, context menu, copy/duplicate, delete
  dialog, recency highlights and drag. The Details column shows the Details
  view directly.
- **BREAKING** Remove the fork-only RPCs `RenameWorkspaceEntry` and
  `CopyWorkspaceEntry` (proto requests, rpc registrations, engine handlers);
  their only consumer was that tree.
- Keep in the upstream explorer the fork's New File / New Folder (helpers move
  from `details_sidebar::file_actions` into `files/`), collapse-all and the
  Changes tab hosting source control. Shared Material file-icon helpers move out
  of `details_sidebar::files_view`.
- Old persisted Details preferences (`active_tab`, `expanded`, `hidden`) are
  ignored on load.
- The source-control surface requirement is restated for its current home, the
  explorer's Changes tab (it still described a Details sidebar tab).

## Capabilities
### Modified Capabilities
- `source-control`: the surface lives in the Files explorer Changes tab.

## Impact
`crates/ui/src/details_sidebar/`, `crates/ui/src/files/`, shell wiring,
`crates/{proto,rpc,engine}` workspace entry RPCs, `ARCHITECTURE.md`,
`DESIGN.md`, `docs/PARITY.md`, `docs/upstream-sync.md`, crate DOX.
