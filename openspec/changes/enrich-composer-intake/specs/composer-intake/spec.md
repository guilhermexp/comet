## ADDED Requirements

### Requirement: Long pasted text becomes a staged attachment

The composer SHALL stage pasted plain text longer than 5 000 characters as a
text attachment (bytes, generated filename, first-line preview) instead of
inserting it into the input, and SHALL deliver it to the run device on send as
a local file whose path is listed in the prompt alongside image attachments.

#### Scenario: Paste over the threshold
Test: UI unit test over the paste-precedence decision; engine attachment test for the delivery rail.

- **WHEN** the user pastes text longer than 5 000 characters
- **THEN** the input text does not change
- **AND** a staged text attachment appears with a first-line preview and size
- **AND** sending delivers the file and lists its local path in the prompt

#### Scenario: Paste at or under the threshold
- **WHEN** the user pastes text of 5 000 characters or fewer
- **THEN** it inserts at the caret as plain text

### Requirement: The input enforces a visible size cap

The composer input SHALL cap total text at 10 000 characters. A paste that
would exceed the cap SHALL be truncated to the available space, and both
truncation and rejection (full input) SHALL surface the composer's failure
notice stating what happened; nothing is dropped silently.

#### Scenario: Paste into a nearly full input
- **WHEN** a paste would push the input past 10 000 characters
- **THEN** only the fitting prefix is inserted
- **AND** the failure notice reports the truncation

#### Scenario: Paste into a full input
- **WHEN** the input is already at the cap
- **THEN** the paste inserts nothing
- **AND** the failure notice says the input is full

### Requirement: Dropped and pasted file paths are classified, never silently discarded

`add_paths` SHALL handle every path: images stage as attachments (current
behavior); non-image files inside the selected space insert a file mention
chip; non-image files outside the space stage as attachments; failures surface
the composer failure notice naming the file.

#### Scenario: Drop a project text file
- **WHEN** the user drops a text file that lives inside the selected space
- **THEN** a file mention chip for its project-relative path is inserted

#### Scenario: Drop an external file
- **WHEN** the user drops a non-image file outside the selected space
- **THEN** it stages as an attachment delivered by path on send

#### Scenario: Drop something unusable
- **WHEN** a dropped path cannot be read or classified
- **THEN** the failure notice names the file instead of silence

### Requirement: Staged non-image items are visible and removable

Every staged text attachment SHALL render in the staged strip as a chip with
an icon, a first-line title, a subtitle carrying its kind and size, and a
remove control, persisting per chat key across navigation exactly like staged
images.

#### Scenario: Review and remove before send
- **WHEN** a text attachment is staged and the user navigates away and back
- **THEN** the chip is still present
- **AND** its remove control deletes only that item

#### Scenario: Failed send restores the stage
- **WHEN** a send carrying staged text attachments fails
- **THEN** the chips return to the strip with the composer text
