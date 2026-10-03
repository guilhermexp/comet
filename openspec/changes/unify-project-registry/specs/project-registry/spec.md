# Spec Delta

## Purpose

Define the single project registry: a project is a Space, every surface registers into and reads from it, and Worker execution checkouts hang off it instead of forming a second registry.

## ADDED Requirements

### Requirement: A project is a Space

Comet SHALL have exactly one project registry: the Space registry. A project's identity SHALL be its Space id, its owner SHALL be the Space's device, and its display name SHALL be the Space's display name. No surface SHALL mint a project identity outside that registry.

#### Scenario: Every project listing agrees
- Test: integration — one fixture registry read through the chat MCP `list_projects`, the Workers controller MCP `list_projects` and the Settings → Projects row builder.
- **WHEN** the chat MCP `list_projects`, the Workers controller MCP `list_projects` and Settings → Projects are read against the same registry
- **THEN** all three contain the same set of project ids
- **AND** each project has the same name, path and device in all three

#### Scenario: A Workers-only project cannot exist
- Test: integration — Workers controller `list_projects` after migration over a state carrying a live `projects[]` entry.
- **WHEN** the Workers state contains a registration with no linked Space
- **THEN** it is not listed as a project
- **AND** it appears only as an association-pending checkout

### Requirement: Every add-project entry point registers a Space

Adding a folder as a project from the Orchestrator palette, the Workers palette, the Settings → Projects add action or the Workers controller MCP `add_project` SHALL create the Space for that folder on the chosen device, or reuse the existing Space for the same device and canonical path. Adding a linked worktree SHALL resolve to the Space of its repository root and register the folder as a checkout of that Space. Adding SHALL be idempotent.

#### Scenario: Adding from Workers creates the Space
- Test: integration — Workers controller MCP `add_project` against a test engine, then chat MCP `list_projects`.
- **WHEN** a folder with no Space is added through the Workers controller MCP `add_project`
- **THEN** the chat MCP `list_projects` returns a project for that folder on the local device
- **AND** the `add_project` response returns that project's Space id

#### Scenario: Adding a folder twice from different entry points
- Test: integration — add the same path through the Orchestrator create-space path and Workers `add_project`.
- **WHEN** the same folder is added from the Orchestrator palette and later from Workers
- **THEN** exactly one project exists for that device and path
- **AND** the second add returns the first project's id

#### Scenario: Adding a linked worktree
- Test: integration — `git worktree add` fixture, then `add_project` on the worktree path.
- **WHEN** the added folder is a linked worktree whose repository root has a Space
- **THEN** no new project is created
- **AND** the folder becomes a checkout of the root's project with its branch

#### Scenario: Adding a linked worktree whose root has no Space
- Test: integration — worktree fixture whose root is not registered.
- **WHEN** the added folder is a linked worktree whose repository root has no Space
- **THEN** a project is created for the repository root
- **AND** the folder becomes a checkout of that project

#### Scenario: The engine is unreachable
- Test: integration — Workers controller `add_project` with no engine endpoint.
- **WHEN** a Workers entry point cannot reach the Space registry
- **THEN** the add fails with an error naming the unreachable registry
- **AND** no Workers-only registration is written

### Requirement: Worker checkouts belong to a project

Each Worker execution checkout SHALL be linked to exactly one project on the local device. The principal checkout of a Git project SHALL be the project's folder; linked worktrees of the same repository SHALL be checkouts of the same project. Existing checkout ids SHALL be preserved so that Worker sessions, restart, ordering and history keep resolving.

#### Scenario: Launching into a project
- Test: integration — Workers controller `launch_worker` with a Space id.
- **WHEN** `launch_worker` names a local project id
- **THEN** the Worker runs in that project's principal folder
- **AND** the session records the principal checkout

#### Scenario: Launching into an exact checkout
- Test: integration — `launch_worker` with an existing `comet-*` checkout id of a linked worktree.
- **WHEN** `launch_worker` names a checkout id
- **THEN** the Worker runs in that exact checkout, not in the principal

#### Scenario: Launching into a remote project
- Test: integration — `launch_worker` with a Space id owned by another device.
- **WHEN** `launch_worker` names a project owned by another device
- **THEN** it fails before spawning, naming the owning device
- **AND** no session is created

#### Scenario: Existing sessions survive the link
- Test: integration — migration over a state with sessions, `session_sort_modes` and session order keyed by `comet-*` ids.
- **WHEN** existing checkouts are linked to projects
- **THEN** every existing session remains listed under its checkout, restartable, and in its previous order

### Requirement: The Workers controller lists projects with their checkouts

The Workers controller MCP `list_projects` SHALL return every project of the registry with id, name, path, device id, device name and Git flag, and SHALL nest, for local projects, each checkout with its checkout id, branch, kind, availability, archived and detached flags.

#### Scenario: Listing mirrors the chat MCP
- Test: integration — Workers controller `list_projects` against a fixture registry with a local and a remote project.
- **WHEN** the registry holds a local project with a linked worktree and a remote project
- **THEN** both projects are returned with the chat MCP's ids and device fields
- **AND** only the local project carries checkouts, including the worktree with its branch

### Requirement: The Workers controller exposes each project's activity

The Workers controller MCP `list_projects` SHALL carry, for every project, the information Settings → Projects shows in its General, Tickets, Worker sessions and Orchestrator sessions tabs, read from the same sources and matching rules those tabs use, so an Orchestrator can decide without opening Settings. Each project SHALL carry: `general` (added and last-opened time, repository remote and default branch when the project is local and they are known); `tickets` (count per status, every `open` ticket and the five most recent others, each with id, title, status, created time, checkout `cwd` and `next`); `worker_sessions` (live and archived counts, every live session and the five most recent archived ones, each with session id, title, checkout id, provider, state, activity and last update); `orchestrator_sessions` (live and archived counts and the five most recent chats, each with chat id, title, archived flag, Workers launched and last activity). Only an unreadable registry SHALL fail the listing. Any other unreadable source, the Workers state included, SHALL leave its section empty with an `error` naming the source; a source read in part SHALL keep what it read and add the `error`. A remote project SHALL carry `worker_sessions` empty, because Worker sessions are device-local.

#### Scenario: A local project with tickets and sessions
- Test: integration — Workers controller `list_projects` against a fixture registry, a fixture Orchestrator workspace (`ORCH_WORKSPACE`) holding tickets for the project's checkout `cwd` and for its harness folder name, Worker sessions on the principal and a worktree, and project chats.
- **WHEN** the project has open and closed tickets, live and archived Worker sessions and chats
- **THEN** its entry carries the ticket counts per status with every open ticket, the Worker session counts with every live session, and the chat counts with the most recent chats
- **AND** the ticket ids, session ids and chat ids are the ones the Settings → Projects tabs list for the same project

#### Scenario: A ticket matches only its own project
- Test: integration — two fixture projects, tickets targeting each by `cwd`.
- **WHEN** tickets target different projects
- **THEN** each project's `tickets` contains only its own tickets

#### Scenario: The Orchestrator workspace is missing
- Test: integration — `ORCH_WORKSPACE` pointing to a missing folder.
- **WHEN** the ticket source cannot be read
- **THEN** `list_projects` still returns every project
- **AND** each `tickets` section is empty and names the unreadable source

#### Scenario: A remote project's activity
- Test: integration — fixture registry with a remote project that has chats.
- **WHEN** the project belongs to another device
- **THEN** its `worker_sessions` is empty and its `orchestrator_sessions` lists its chats

### Requirement: Existing Workers records migrate into the registry

On first start after upgrade, Comet SHALL link every Workers registration and ledger record to a project, once and idempotently, after writing a backup of the Workers state. A record SHALL be linked to the local project whose folder is its repository root, or to the local project of its own folder when it is not in a Git repository; when that project does not exist it SHALL be created. A record with no filesystem and no persisted Git evidence SHALL stay association-pending and SHALL NOT be linked by name, path prefix or remote.

#### Scenario: Worktrees whose principal was not registered
- Test: integration — migration over a fixture repository with three registered linked worktrees and no registered principal.
- **WHEN** migration runs
- **THEN** one project is created for the repository root
- **AND** the three worktrees are checkouts of that project
- **AND** no "Principal not registered" container exists

#### Scenario: A registration that already has a Space
- Test: integration — migration where a Space already exists for the registration's path.
- **WHEN** a Workers registration's folder already has a local Space
- **THEN** the registration is linked to that Space and no Space is created

#### Scenario: Running migration again
- Test: integration — run migration twice over the same state.
- **WHEN** migration runs a second time
- **THEN** it creates no project, changes no link, and writes no new backup

#### Scenario: A record without evidence
- Test: integration — ledger-only record whose folder is gone and whose identity has no repository.
- **WHEN** a record has no folder and no persisted repository membership
- **THEN** it stays association-pending and reachable in Settings → Projects

### Requirement: Registry consumers read the unified registry

The Workers sidebar, the Workers project filter, composer `@` project mentions and Source Control checkout authorization SHALL resolve projects from the Space registry and checkouts from their project link. Source Control SHALL authorize a Worker checkout because it belongs to a local project, not because it appears in the Workers state.

#### Scenario: Source Control in a Worker worktree
- Test: integration — engine checkout status RPC on a linked worktree checkout of a local project.
- **WHEN** Source Control is requested for a Worker worktree of a local project
- **THEN** it is authorized

#### Scenario: Source Control in an unlinked folder
- Test: integration — engine checkout status RPC on a folder with no project.
- **WHEN** Source Control is requested for a folder that belongs to no local project
- **THEN** it is rejected as not a known checkout on this device

#### Scenario: Mentioning a project
- Test: unit — composer project mention source.
- **WHEN** the user types `@` in the composer
- **THEN** the offered projects are the registry's projects with their device
