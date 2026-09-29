# Spec Delta

## MODIFIED Requirements

### Requirement: Shared project palette
Worker project addition SHALL use the same in-app folder palette as Orchestrator, retaining folder search, keyboard navigation, hidden folders and locations. Confirmation SHALL create or reuse the project (Space) for the current folder on the local device and register its checkout, without launching a Worker. Cancellation SHALL leave the registry unchanged.

#### Scenario: Open and cancel Worker project selection
- Test: none — native GPUI QA; no automated render harness.
- **WHEN** the user opens New project or Command-K in Workers
- **THEN** the shared folder palette opens and Escape dismisses without registration

#### Scenario: Confirm local folder
- Test: integration — Workers add-project path, then chat MCP `list_projects`; native GPUI QA for selection.
- **WHEN** the user confirms the browsed folder in Worker mode
- **THEN** that folder's project is created or reused on the local device and becomes the selected Worker project
- **AND** the same project appears in the Orchestrator project list
