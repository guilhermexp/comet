# native-preview-focus Specification

## Purpose
Keep native keyboard routing connected to the Chat UI when an embedded WebKit preview closes, while preserving focus owned by unrelated controls.

## Requirements

### Requirement: Restore native keyboard ownership on preview detach
Closing a native HTML, PDF, or video preview SHALL return AppKit keyboard ownership to the GPUIView in the same window when the preview or its descendant owns first responder. It SHALL preserve an unrelated responder and report restoration failure.

#### Scenario: Preview owns the keyboard
Test: integration — `ZERON_NATIVE_PREVIEW_FOCUS_TEST=1 cargo test -p zeron-ui --test native_preview_focus` (macOS main-thread GPUI window and WebKit).
- **WHEN** a focused native preview is hidden or dropped
- **THEN** the window's first responder is its GPUIView, rather than the contentView container
- **AND** repeated detach remains safe

#### Scenario: Another view owns the keyboard
Test: integration — `ZERON_NATIVE_PREVIEW_FOCUS_TEST=1 cargo test -p zeron-ui --test native_preview_focus`.
- **WHEN** a native preview is detached while an unrelated view owns first responder
- **THEN** the unrelated responder remains unchanged
