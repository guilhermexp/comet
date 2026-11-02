## ADDED Requirements

### Requirement: Custom worktree location is persisted transactionally
Settings General SHALL present the custom worktree location as enabled only when that choice and a usable folder are persisted; a first enable obtains the folder before saving, and cancellation or failure keeps the persisted state. The saved choice and folder survive restart. The app SHALL NOT force it on, pick a folder, or move or delete worktrees.

#### Scenario: First enable with folder choice
Test: unit — settings model and engine persistence fixtures.
- **WHEN** no directory is remembered and the user enables the switch and confirms a valid folder
- **THEN** the enabled preference with that folder is persisted before the switch is presented as on
- **AND** reloading the settings presents the same enabled folder

#### Scenario: Cancelled enable
Test: unit — settings model fixture.
- **WHEN** no directory is remembered and the user enables the switch and cancels or the folder is invalid
- **THEN** the switch is presented with the previously persisted state
- **AND** no preference file is written

#### Scenario: Explicit disable survives restart
Test: unit — engine persistence fixture.
- **WHEN** the user disables a previously enabled location and the settings are reloaded
- **THEN** the location is disabled and the remembered folder is retained for a later enable

### Requirement: Saved worktree root drives Chat and Worker creation
The effective root for new Chat worktrees and new Worker checkouts SHALL be derived from the saved preference, preserving the documented environment override precedence and the default root when the preference is disabled or absent.

#### Scenario: Worker and Chat creation use the saved root
Test: integration — temporary Git repositories and synthetic settings.
- **WHEN** a synthetic settings file enables a custom root and a Chat worktree and a Worker checkout are created from a temporary repository
- **THEN** both checkouts are created under the custom root
- **AND** an explicit environment override still takes precedence
- **AND** a disabled preference uses the default root

#### Scenario: One creation resolves the root once
Test: integration — temporary repositories with the preference switched mid-creation.
- **WHEN** the saved root changes after a Worker creation resolved its root but before its ownership journal validates the destination
- **THEN** that checkout is created and owned under the root it resolved
- **AND** the next creation uses the new root

#### Scenario: Workers read the app's data dir on Windows
Test: unit — pure resolver with Windows and Unix inputs.
- **WHEN** neither `ZERON_DATA_DIR` nor `LOCALAPPDATA` is set (or both are empty) on Windows
- **THEN** Workers read the saved preference from `%USERPROFILE%\AppData\Local\Zeron`, the same directory the app uses, not `~/.zeron`
