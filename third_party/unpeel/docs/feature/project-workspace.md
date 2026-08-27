# Project Workspace

## Summary

Unpeel organizes sessions around projects and workspaces rather than around a flat terminal list.

This is the main app-level organization layer on top of the hosted session backend.

## User Behavior

- Projects can be grouped into workspaces.
- Each project keeps its own active tab.
- Sidebar pins can point to sessions or recent-session entries.
- Recent history and saved sessions stay scoped to the project.

## Main Files

- [project.rs](/Users/tommyvedvik/Dev/unpeel/apps/desktop/src-tauri/src/project.rs)
- [workspace.rs](/Users/tommyvedvik/Dev/unpeel/apps/desktop/src-tauri/src/workspace.rs)
- [sidebar.rs](/Users/tommyvedvik/Dev/unpeel/apps/desktop/src-tauri/src/sidebar.rs)
- [sessions.ts](/Users/tommyvedvik/Dev/unpeel/apps/desktop/src/lib/stores/sessions.ts)
- [SessionLauncherView.svelte](/Users/tommyvedvik/Dev/unpeel/apps/desktop/src/lib/SessionLauncherView.svelte)

## Stored State

App-level workspace and project state lives in:

- `~/.unpeel/app-state.json`

Important state includes:

- projects
- workspaces
- active tabs
- pinned sidebar entries
- presets
- saved sessions
- recent sessions

## Why It Matters

This is what makes Unpeel a session manager for AI CLIs instead of just another terminal window.
