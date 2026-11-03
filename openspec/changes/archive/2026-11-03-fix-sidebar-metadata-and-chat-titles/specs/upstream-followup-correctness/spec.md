# Spec Delta

## MODIFIED Requirements

### Requirement: Restricted Chat titles

Chat titles SHALL use an isolated, bounded, tool-free run with validated device-local harness/model preferences and a safe fallback. If no usable generated title is obtained, the Chat SHALL remain eligible for a later normal title-generation trigger instead of persisting prompt words as a definitive title. A failed generation SHALL NOT rename its worktree branch.

#### Scenario: Restricted Chat titles regression

- **WHEN** a title run requests a tool, times out, or has an unsupported configuration
- **THEN** no tool runs and title generation falls back without modifying project files
- **Test:** unit / integration

#### Scenario: Failed generation does not permanently name the Chat
Test: unit — isolated engine/workspace and controlled title harness.

- **WHEN** an untitled Chat receives no usable generated title after the existing bounded attempts
- **THEN** no definitive title is written and no automatic worktree rename occurs
- **AND** the interface retains its existing untitled placeholder

#### Scenario: A later normal trigger can generate a title after recovery
Test: unit — failure followed by success through the existing generator and workspace.

- **WHEN** generation previously failed for an untitled Chat and a subsequent normal generation trigger obtains a valid title
- **THEN** that generated title is persisted
- **AND** the earlier failure does not cause the later attempt to be skipped as already named
- **AND** this change does not add timers, a new retry ladder or provider switching

## ADDED Requirements

### Requirement: Automatic titles preserve explicit and legacy names

Automatic title generation SHALL apply a generated result only while the Chat still exists and remains untitled at the moment of mutation. Existing titles, including legacy names whose origin is unknown, SHALL be preserved without guessing whether they were manual or fallback. No migration or bulk rename SHALL reinterpret existing title text.

#### Scenario: A Chat already has a title
Test: unit — actual title generator and isolated workspace.

- **WHEN** generation is requested for a Chat with a nonempty existing title
- **THEN** no title model is invoked and the existing title is unchanged
- **AND** a legacy title matching the initial prompt words receives the same protection

#### Scenario: User names the Chat while generation is in flight
Test: unit — controlled pending harness and conditional workspace mutation.

- **WHEN** a manual rename is committed before the automatic result is applied
- **THEN** that manual title is retained
- **AND** the automatic result does not overwrite it

#### Scenario: Chat is removed before the generated result returns
Test: unit — controlled pending harness and isolated workspace removal.

- **WHEN** the Chat is removed while title generation is pending
- **THEN** the automatic result does not recreate or rename that Chat
