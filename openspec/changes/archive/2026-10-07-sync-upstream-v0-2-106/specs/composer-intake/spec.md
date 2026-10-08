## ADDED Requirements

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
