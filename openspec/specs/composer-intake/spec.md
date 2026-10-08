# composer-intake Specification

## Purpose

Staging and delivery of long pasted text and files as prompt attachments in the composer.

## Requirements

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
Test: unit — composer.rs paste-decision tests (`paste_decision_*`).

- **WHEN** the user pastes text of 5 000 characters or fewer
- **THEN** it inserts at the caret as plain text

### Requirement: The input enforces a visible size cap

The composer input SHALL cap total text at 10 000 characters. A paste that
would exceed the cap SHALL be truncated to the available space, and both
truncation and rejection (full input) SHALL surface the composer's failure
notice stating what happened; nothing is dropped silently.

#### Scenario: Paste into a nearly full input
Test: unit — composer.rs cap/truncation tests over the pure paste decision.

- **WHEN** a paste would push the input past 10 000 characters
- **THEN** only the fitting prefix is inserted
- **AND** the failure notice reports the truncation

#### Scenario: Paste into a full input
Test: unit — composer.rs full-input rejection test.

- **WHEN** the input is already at the cap
- **THEN** the paste inserts nothing
- **AND** the failure notice says the input is full

### Requirement: Dropped and pasted file paths are classified, never silently discarded

`add_paths` SHALL handle every path: images stage as attachments (current
behavior); non-image files inside the selected space insert a file mention
chip; non-image files outside the space stage as attachments; failures surface
the composer failure notice naming the file.

#### Scenario: Drop a project text file
Test: unit — add_paths classification tests (project-relative text file → mention).

- **WHEN** the user drops a text file that lives inside the selected space
- **THEN** a file mention chip for its project-relative path is inserted

#### Scenario: Drop an external file
Test: unit — add_paths classification tests (external path → staged attachment).

- **WHEN** the user drops a non-image file outside the selected space
- **THEN** it stages as an attachment delivered by path on send

#### Scenario: Drop something unusable
Test: unit — add_paths classification tests (unreadable path → failure notice).

- **WHEN** a dropped path cannot be read or classified
- **THEN** the failure notice names the file instead of silence

### Requirement: Staged non-image items are visible and removable

Every staged text attachment SHALL render in the staged strip as a chip with
an icon, a first-line title, a subtitle carrying its kind and size, and a
remove control, persisting per chat key across navigation exactly like staged
images.

#### Scenario: Review and remove before send
Test: unit — composer staged-strip persistence/removal tests.

- **WHEN** a text attachment is staged and the user navigates away and back
- **THEN** the chip is still present
- **AND** its remove control deletes only that item

#### Scenario: Failed send restores the stage
Test: unit — composer.rs restore-on-failed-send test (`pasted_text_stages_bytes_name_preview_and_survives_restore_merge`).

- **WHEN** a send carrying staged text attachments fails
- **THEN** the chips return to the strip with the composer text

### Requirement: Attachment rows wrap within the available column
Staged and sent attachments SHALL wrap within their available column without clipping later attachments or requiring horizontal scrolling. Staged attachments SHALL remain above the input pill.

#### Scenario: Narrow column with multiple attachments
Test: none — native render acceptance at narrow and wide window widths.
- **WHEN** attachments exceed one row in the composer or a sent user message
- **THEN** every attachment remains visible on subsequent rows
- **AND** the input pill keeps its independent height

### Requirement: Attachment references participate in composer editing
Staged images and external files SHALL be referenceable as named inline chips at the caret. Removing a reference or its staged tile SHALL remove the associated staging entry, and undo SHALL restore the association. Sending SHALL resolve chip text to provider-readable labels and deliver file bytes through the existing attachment rail. Project-relative file mentions and long-paste staging SHALL retain their existing classification and precedence.

#### Scenario: Stage edit remove and undo
Test: unit — composer chip/staging history and attachment delivery fixtures; none — headed layout acceptance.
- **WHEN** the user attaches an image or external regular file and edits its inline reference
- **THEN** staging and text remain associated through deletion and undo
- **AND** sending includes readable labels and the delivered local attachment paths
- **AND** dropping a non-image project file retains its project-relative mention behavior

#### Scenario: Colliding external filenames remain independent
Test: unit — executable attachment staging, queue edit, desktop transcript and mobile projection fixtures.
- **WHEN** two staged external files share a basename or collide after filename sanitization
- **THEN** each chip identifies its own upload through send, queue editing and desktop/mobile transcript projection
- **AND** removing or opening one chip affects only its own attachment

#### Scenario: Failure restores attachments staged during send
Test: unit — composer pending-send and failed-send restoration fixtures.
- **WHEN** one send is pending and another attachment is staged before that send fails
- **THEN** the old and new attachment references retain distinct identities after restoration
- **AND** removal, preview and undo affect the intended item only

### Requirement: Dropped attachments enforce size limits and report all refusals
External file attachments SHALL enforce the same 24 MiB staging limit as image attachments. A mixed drop batch SHALL retain every synchronous classification failure and asynchronous staging failure in its visible notice while adopting valid attachments into the captured draft once. Project-relative mentions SHALL retain their classification precedence.

#### Scenario: Refuse an oversized external attachment
Test: unit — shell::chat_dropzone_tests::external_file_drops_stage_files_and_refuse_folders_and_oversize_files.
- **WHEN** the user drops an external regular file larger than 24 MiB
- **THEN** that file does not enter the staged attachment list
- **AND** the failure notice identifies the file and the size limit

#### Scenario: Preserve all refusals in a mixed drop batch
Test: unit — shell::chat_dropzone_tests::external_file_drops_stage_files_and_refuse_folders_and_oversize_files.
- **WHEN** one batch contains valid attachments, a folder and an oversized external file
- **THEN** valid attachments are staged once in the intended draft
- **AND** the visible failure notice retains both the folder refusal and the oversized-file refusal
