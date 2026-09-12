## Context
The shell owns AddSpaceFlow, folder/drive RPCs and its palette renderer. WorkersSidebar currently opens prompt_for_paths and submits to WorkersModel.add_project.

## Decisions
Emit a sidebar event to open the shell palette. Keep an explicit destination for the lifetime of the palette. Worker mode permits only the known local device and dispatches the confirmed folder to the existing Worker action queue. Orchestrator keeps Space creation and remote browsing. Cancellation performs no registration.

## Validation seams
Device eligibility is pure and testable: Worker requests must reject remote/unknown identity; Orchestrator retains all devices. Existing folder navigation tests remain applicable. Entry points, focus, shared rendering and registration are native QA seams.
