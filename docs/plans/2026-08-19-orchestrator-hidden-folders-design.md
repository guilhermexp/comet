# Orchestrator Hidden Folders Design

## Goal

Allow the Orchestrator folder picker to browse and select hidden directories such as `.config`, `.codex`, and `.orchestrator`.

## Product contract

- Hidden directories are always visible; there is no toggle or alternate mode.
- Ordinary files remain excluded from the navigable folder rows.
- Hidden directories participate in the existing case-insensitive ordering, filtering, keyboard navigation, slash descent, and selection behavior.
- Repository detection remains the existing `.git` directory probe.
- Remote-device folder browsing receives the same listing behavior because the change stays at the engine boundary.

## Implementation

Remove only the leading-dot exclusion from `list_folders_blocking`. Keep the existing directory/file metadata contract unchanged so the GPUI folder picker needs no special handling.

## Testing

Update the existing folder-lister integration test to require `.hidden` in the returned directory list while continuing to verify directory-first ordering, repository flags, ordinary-file metadata, and the truncation contract.
