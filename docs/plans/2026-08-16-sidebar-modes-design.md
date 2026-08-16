# Sidebar modes design

## Goal

Add a two-option segmented control above the existing sidebar. The default
option is **Orchestrator** and keeps the current sidebar content unchanged.
Selecting **Workers** keeps the segmented control visible and leaves the rest
of the sidebar empty.

## Design

The selected mode is session-local state owned by `Shell`; it is not persisted
and does not change application routes or settings. The selector is rendered by
the sidebar container so it remains available above both states. The existing
chat or settings sidebar is mounted only while Orchestrator is selected.

The control follows the supplied reference: two equal-width buttons inside a
rounded inset container, with the selected option using the stronger text and
surface treatment. Both buttons have stable element IDs for interaction and
visual verification.

## Alternatives considered

- Persist the selected mode in `UiSettings`: rejected because persistence was
  not requested and would expand the feature surface.
- Model each mode as a route: rejected because sidebar selection should not
  replace or pollute the main content navigation.
- Keep a transient `SidebarMode` on `Shell`: selected for minimal scope and
  predictable startup in Orchestrator.

## Testing

Pure tests cover the default mode and whether each mode exposes the existing
sidebar content. Existing UI tests and formatting remain green. Manual
verification confirms the selector appearance and that Workers is empty.
