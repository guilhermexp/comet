# Fix native preview keyboard focus

## Why
Closing HTML previews restores AppKit first responder to the window container instead of its GPUIView child. GPUI logical focus cannot repair that native routing error.

## What Changes
- Restore the owning window's GPUIView before detaching a focused native preview.
- Preserve unrelated native responders and report failed restoration.
- Exercise the production detach code in an opt-in native macOS integration test.

## Impact
- Affected code: file_preview/native_document.rs.
- No engine, protocol, or preview layout changes.
