# Design: Worked Projects in Details Sidebar

## Architecture

The Worked Projects feature connects three layers without introducing cross-layer leakage:
1. **Harness Normalization**: Maps agent tool execution events into typed `ToolCall` variants in `zeron-harness`.
2. **Pure Derivation**: Extracts candidate paths from assistant tool calls and matches against leaf registered projects in `zeron-ui::details_sidebar::worked_projects`.
3. **UI Rendering**: Renders the collapsible list inside the Workspace widget card in `zeron-ui::details_sidebar::view`.

```
SessionMessageEntry[] (Chat Transcript) ──┐
WorkersProject[] (Registered Projects)  ──┼──> worked_projects() ──> Vec<WorkedProject>
DetailsContext.cwd (Chat Checkout)     ──┤       (Pure Derivation)             │
Home Directory (dirs_home)              ──┘                                     │
                                                                                ▼
                                                                     Workspace Widget Card
                                                                     (Projects worked list)
```

## Pure Derivation Algorithm

1. **Empty inputs**: Return `Vec::new()` if transcript or projects list is empty.
2. **Registered project roots**:
   - Filter `projects` with `is_group == false` and non-empty path.
   - Strip trailing slashes (`/`).
3. **Leaf Root filtering**:
   - For each registered root $R$, discard $R$ if there is another registered root $O$ such that $O \neq R$ and $O$ starts with `$R/` (is strictly within $R$).
4. **Chat Checkout exclusion**:
   - From the surviving leaf roots, exclude the candidate whose path equals `own_checkout` (comparing normalized paths without trailing slashes).
   - If no candidate leaf roots remain, return `Vec::new()`.
5. **Transcript scanning**:
   - Iterate through entries in chronological order.
   - Consider only entries with `role == "assistant"`.
   - For each `MessagePart::Tool { call, .. }`, extract path candidates:
     - `ReadFile { path }`, `WriteFile { path, .. }`, `EditFile { path, .. }` -> single path `path` (structured, trailing punctuation preserved).
     - `ApplyPatch { path: Some(p) }` -> single path `p` (structured, trailing punctuation preserved).
     - `Search { path: Some(p), .. }` -> tokens scanned from `p` via `scan_path_tokens` (trailing punctuation stripped).
     - `Glob { pattern }` -> tokens scanned from `pattern` via `scan_path_tokens` (trailing punctuation stripped).
     - `Exec { command }` -> all absolute or home-relative path tokens scanned from `command` via `scan_path_tokens` (trailing punctuation stripped).
     - Other variants -> ignored.
6. **Path Token Scanner (without regex)**:
   - Scans tokens starting with `/` or `~/` and ending at the first whitespace (`char::is_whitespace`), single quote (`'`), double quote (`"`), backtick (`` ` ``), or closing parenthesis (`)`).
7. **Path candidate normalization**:
   - `trim()` whitespace.
   - For tokens from scanner: strip repeated trailing punctuation: `)`, `.`, `,`, `;`, `:`.
   - Strip trailing slashes `/`.
   - Discard if not starting with `/` or `~/`.
   - If starting with `~/`:
     - If `home_dir` is `Some(h)`: replace `~` with `h` path string.
     - If `home_dir` is `None`: discard candidate.
8. **Matching and first-contact ordering**:
   - For each candidate path, check each candidate leaf root:
     - Matches if `candidate == root` or `candidate.starts_with(&format!("{root}/"))` (enforcing component boundaries).
   - Record monotonic order counter on first match.
   - Short-circuit when all candidate leaf roots have matched.
9. **Output**:
   - Sort matched projects by ascending first-contact order.
   - Map to `WorkedProject { id, name, path }` preserving original `WorkersProject.path`.
## UI Rendering in Workspace Card

- Rendered in `render_details` below `Path` row inside the Workspace `widget_card`.
- Gated to `DetailsMode::Orchestrator` and `!worked_projects.is_empty()`.
- Header:
  - Folder icon (`icons::FOLDER`), label "Projects worked", count `worked_projects.len()`, collapse chevron (`ALT_ARROW_DOWN` / `ALT_ARROW_RIGHT`).
  - Toggles persistent collapse state in `DetailsSidebarPreferences.expanded`.
- Body (when expanded):
  - Bounded viewport with `max_h` (e.g. 5 visible rows max) and `overflow_y_scroll`.
  - Fixed-height row per worked project with folder icon and truncated single-line name.
  - Left click calls `WorkersModel::reveal_project(project.path, cx)`.
