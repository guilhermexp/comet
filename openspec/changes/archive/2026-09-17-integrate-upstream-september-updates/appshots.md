# Appshots integration plan and evidence

## Scope

This slice adapts the Appshots portion of upstream `0a80fc15` to the fork's
Composer, Shell, headed service, and iOS transcript surfaces. It preserves the
fork's attachment transport, steering behavior, native preview, and existing
Composer layout.

## Implementation plan

1. Capture the destination Chat before the native capture task awaits, then
   deliver the completed capture or its error to that immutable target even if
   navigation changes during capture. Delivery returns to the Orchestrator
   surface before selecting a Chat or opening the new-Chat canvas.
2. Stage Appshots per Chat with a process-wide retained-byte budget, render
   source-labelled cards, and upload their screenshots with ordinary
   attachments while retaining escaped semantic context in the prompt.
3. Restore Appshots as Appshots after a failed send and remove them with the
   deleted Chat's in-memory staging state. Remember the Last Chat only after
   accepted staging (or the optimistic send of a new-Chat Appshot), clear it on
   Chat deletion, and keep Chat-scoped capture failures attached to their
   original target. The fork has no queued-message edit lease or queue editor,
   so upstream queue-edit restoration is not included.
4. Present Appshot metadata on iOS by parsing bounded untrusted context,
   suppressing it from visible prompt text, and reusing the relay attachment
   cache/lightbox for source-labelled cards.
5. Document the capture seam and add focused Rust and XCTest coverage. Native
   capture permission and Xcode validation remain platform-gated.

## Adaptations

- The fork's Shell service passes an `Option<String>` target into delivery;
  `Automatic` uses the selected Chat or a new-Chat canvas, while
  `LastSession` may fall back to the last Appshot Chat.
- The desktop error path carries that same target into the Composer. The
  Orchestrator surface is selected before delivery, and new-Chat Appshots
  update the Last Chat pointer from the Composer `Sent` event.
- Appshot screenshots reuse `StagedAttachment` and attachment cache paths;
  semantic metadata is kept separately so a failed upload does not turn a
  capture into an ordinary image chip.
- Neither desktop nor iOS has the upstream queued-message edit surface in this
  checkout. The existing queued-send `pending://` flow still carries Appshot
  context; visible text parsing strips that context safely for every user
  message.

## Evidence

- `crates/ui/src/appshots/AGENTS.md` records capture permissions, limits,
  target immutability, and the local test matrix.
- `crates/ui/src/{lib,shell,composer}.rs` implement target capture, staging,
  rendering, ordinary upload reuse, semantic context, and failure restore.
- `apps/ios/Zeron/Composer/Appshots.swift` and `Attachments.swift` render
  source-labelled cards while hiding observed semantic text.
- `apps/ios/ZeronTests/AppshotTests.swift` covers source parsing, malformed /
  duplicate / DTD context rejection, and ordinary attachment compatibility.

## Validation log

- `cargo check -p zeron-ui` — passed; final native build is warning-free.
- `cargo test -p zeron-ui appshots --lib` — **20 passed** (native macOS
  capability/shortcut cases, bounded capture, escaped context, presentation
  metadata, Chat terminology, duplicate rejection, and padding normalization).
- `cargo test -p zeron-ui composer --lib` — **84 passed** (send/failure
  recovery, attachment staging, focus, layout and existing composer paths).
- `cargo test -p zeron-ui shell::tests::appshot_destination_is_resolved_before_capture --lib`
  — **1 passed**.
- `cargo test -p zeron-ui --lib --no-fail-fast` — **1,393 passed, 0 failed**.
- `swiftc -parse apps/ios/Zeron/Composer/Appshots.swift
  apps/ios/Zeron/Composer/Attachments.swift
  apps/ios/ZeronTests/AppshotTests.swift` — passed.
- `git diff --check` — passed for the scoped Rust/Swift/DOX paths.
- Full `xcodebuild` and simulator UI capture are unavailable here: only
  Command Line Tools are installed and Xcode/simctl are absent.
