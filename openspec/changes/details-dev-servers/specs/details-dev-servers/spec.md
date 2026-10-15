## ADDED Requirements

### Requirement: Details lists servers running in the chat's projects

Details SHALL show a Servers section listing development servers whose working directory is inside the chat's folder or a Project worked, on the chat's host device, one row per process and port. A listener SHALL count as a development server only when it or a launcher above it (up to a shell or agent CLI) runs a dev command such as `npm run dev`, `next dev`, `vite`, `cargo run` or `mix phx.server`; databases, caches, agents and browsers SHALL NOT appear. Only servers the chat's agent started SHALL be shown: one of the chat's own shell commands names the server's port or runs one of its dev commands as whole words. The list SHALL show about three rows and scroll inside the card. The section SHALL be hidden when nothing qualifies.

#### Scenario: Agent starts a dev server in a Project worked
- **Test:** unit
- **WHEN** a chat in `Orchestrator.v3` worked on `bops` and `next dev --port 3210` listens with cwd `bops`
- **THEN** Servers shows `:3210 · next dev · bops`

#### Scenario: Services the agent did not start
- **Test:** unit
- **WHEN** postgres, redis, minio and the agent's own ports listen inside a Project worked
- **THEN** none of them appears in Servers

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
