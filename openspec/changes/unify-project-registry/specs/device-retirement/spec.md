# Spec Delta

## Purpose

Let the owner retire a stale duplicate device so only current devices remain, moving its projects and their chats onto the local device without losing ids or history.

## ADDED Requirements

### Requirement: A stale device can be retired

Settings → Devices SHALL offer a retire action for a device that is not the local device and is not online. Retiring SHALL re-home the device's projects and then remove the device from every device listing on every synced device. The local device and online devices SHALL NOT be retirable.

#### Scenario: Retiring the legacy duplicate
- Test: integration — engine with a local device and an offline device owning one Space with chats; retire through the RPC surface.
- **WHEN** the owner retires an offline, non-local device
- **THEN** the device no longer appears in Settings → Devices or in the chat MCP `list_devices`
- **AND** its projects appear on the local device

#### Scenario: The local or an online device
- Test: unit — retire eligibility.
- **WHEN** the device is the local device or was seen within the online window
- **THEN** the retire action is unavailable and the RPC rejects it

#### Scenario: A retired device reconnects
- Test: integration — a tombstoned device id pushes a presence/upsert.
- **WHEN** a retired device's engine connects again
- **THEN** it is not listed again
- **AND** the projects moved away from it stay on the local device

### Requirement: Retirement re-homes projects and chats without loss

For each project owned by the retiring device, Comet SHALL move it to the local device when its folder exists on the local device, keeping its id, name and creation date, and SHALL move every chat of that project to the local device, keeping chat ids, transcripts, archive state and trajectory. When any such folder does not exist locally, retirement SHALL be refused before any change, listing the missing folders.

#### Scenario: Chats follow their project
- Test: integration — retire a device whose Space has active and archived chats.
- **WHEN** the device is retired
- **THEN** every chat of its projects reports the local device
- **AND** each chat keeps its id, title, archive state and transcript
- **AND** a new run in one of those chats executes on the local device

#### Scenario: A folder missing locally
- Test: integration — retire a device whose Space path does not exist on the local device.
- **WHEN** one of the device's project folders is missing locally
- **THEN** retirement fails naming that folder
- **AND** no project, chat or device changes

#### Scenario: A folder that already has a local project
- Test: integration — retiring device owns path P; local device already owns a Space for P.
- **WHEN** the local device already has a project for the same folder
- **THEN** retirement fails naming the conflicting projects
- **AND** no project, chat or device changes
