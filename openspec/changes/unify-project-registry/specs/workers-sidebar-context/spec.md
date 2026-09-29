# Spec Delta

## ADDED Requirements

### Requirement: Workers base projects are local projects of the registry

The Workers sidebar SHALL use the registry's projects owned by the local device as its base projects, named as in the registry, with each project's linked checkouts nested under it. A Workers registration without a project SHALL appear only in an association-pending container.

#### Scenario: Worktrees of an unregistered principal
- Test: unit — project tree projection over a project whose principal has no execution registration.
- **WHEN** a local project has linked worktree checkouts and no principal registration
- **THEN** the base row is the project with its registry name
- **AND** no "Principal not registered" container is shown

#### Scenario: Projects of other devices
- Test: unit — project tree projection over a registry with a remote project.
- **WHEN** the registry holds a project owned by another device
- **THEN** it does not appear as a Workers base project
