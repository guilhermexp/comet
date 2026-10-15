## ADDED Requirements

### Requirement: Details lists servers running in the chat's projects

Details SHALL show a Servers section listing every TCP listener whose working directory is inside the chat's folder or a Project worked, on the chat's host device, one row per process and port. Zeron's own engine and authentication callbacks SHALL be excluded. The section SHALL be hidden when nothing listens.

#### Scenario: Agent starts a dev server in a Project worked
- **Test:** unit
- **WHEN** a chat in `Orchestrator.v3` worked on `bops` and `next dev --port 3210` listens with cwd `bops`
- **THEN** Servers shows `:3210 · next-server · bops`

### Requirement: A listed server can be stopped from Details

Hovering a server row SHALL reveal Stop. The host SHALL refuse to stop a process that no longer has the listed start time or is no longer a listener under the given root. It SHALL send SIGTERM to the listener and to its ancestors whose working directory is inside the same root, stopping at a shell, an agent CLI, Zeron or after three levels.

#### Scenario: Stop a Next.js dev server
- **Test:** unit
- **WHEN** the user stops `:3210`, launched as `node …/next dev` → `next-server`
- **THEN** both `next dev` and `next-server` receive SIGTERM and the shell that started them does not

#### Scenario: Recycled pid
- **Test:** unit
- **WHEN** the listed pid now belongs to another process
- **THEN** the stop is refused and nothing is signalled
