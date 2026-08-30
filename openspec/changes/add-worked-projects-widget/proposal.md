# Change: Worked Projects widget in Details sidebar

## Why

In Orchestrator mode, an orchestrator chat runs in its private workspace but performs its actual development work across registered project checkouts. Users need immediate visibility into which registered projects a given chat session touched without manually reading the entire transcript.

The reference implementation in Orchestrator.dev provides a "Projects worked" section in the Workspace card of the Details sidebar directly below the `Path` row, showing the count, a collapsible list of registered projects touched by the chat's assistant tool calls in chronological order of first contact, and click-to-reveal in Finder.

This change brings parity to Comet by introducing the pure derivation `worked_projects`, tool-call normalization parity for `grep` and `glob` in the OMP harness, and rendering the collapsible "Projects worked" block in the Workspace card.

## Decisions

- **D-01: Pure derivation over Chat Transcript and Registered Projects.** `worked_projects` derives from `&[SessionMessageEntry]`, `&[WorkersProject]`, the chat's own checkout `&Path`, and an optional home directory `Option<&Path>`. It performs no I/O, does not spawn processes, and reads only assistant tool calls from the current chat.
- **D-02: Strict Signal boundary (S1).** Only assistant tool calls from the chat itself are considered (`ReadFile`, `WriteFile`, `EditFile`, `ApplyPatch`, `Search`, `Glob`, `Exec`). Dispatched workers, child subagent documents (`subagent_ref`), and user messages are excluded.
- **D-03: Registered Projects universe (S2).** Only registered projects (`WorkersModel::projects()`) can be matched. Unregistered paths touched by the agent never appear.
- **D-04: Mandatory Leaf Root filtering (S3).** A registered project that is a strict ancestor of another registered project is filtered out from prefix matching to prevent broad container roots from absorbing every path.
- **D-05: Chat Checkout exclusion (S4).** The chat's own checkout (`DetailsContext.cwd`) is excluded from candidate roots because it is already displayed in the `Path` row above.
- **D-06: Absolute paths only (S5).** Relative paths are ignored; only tokens starting with `/` or `~/` are considered.
- **D-07: First-contact chronological ordering (S6).** Projects are ordered ascending by the monotonic order of first contact in the transcript.
- **D-08: Visibility gated to Orchestrator mode and non-zero count (S7).** The widget is rendered only when `DetailsMode::Orchestrator` is active and the worked project count is greater than zero.
- **D-09: Harness normalization parity for grep/glob.** The OMP harness normalizer maps `grep` to `ToolCall::Search` and `glob` to `ToolCall::Glob` (extracting the pattern from the `path` field as specified by the OMP protocol) so paths are not sanitized away as unknown tools.
- **D-10: Persistent collapse state via existing DetailsSidebarPreferences.** Collapse state is stored keyed by the context key in `DetailsSidebarPreferences.expanded` using a dedicated identifier, avoiding new settings files or preference schema changes.

## Non-goals

- Deriving worked projects from dispatched worker rows or `ChatWorkerRow.project_id`.
- Loading subagent transcripts from `subagent_ref`.
- Adding new dependencies (e.g. `regex`).
- Relaxing `sanitize_tool_call` privacy policies for unknown or MCP tools.
- Creating new `DetailsSidebarEvent` variants or custom settings stores.
