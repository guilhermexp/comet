# Appshots — native window captures

Pai: [`../../AGENTS.md`](../../AGENTS.md)

## Purpose

`appshots.rs` owns the headed desktop capture seam: global shortcut
registration, bounded screenshot acquisition, optional accessibility context,
and the transport metadata used by the Composer. The platform modules under
this directory are replaceable backends; they never decide which Chat receives
a capture.

## Ownership

This seam owns capture permissions, native backend capability reporting,
capture-size limits, semantic-context escaping/restoration, and the shortcut
preference stream. `Shell` owns destination selection at shortcut time and
`Composer` owns per-Chat staging, upload, rendering, and failure recovery.

## Local Contracts

- A capture is user-triggered and remains on the headed/viewer device. The
  screenshot is transported through the ordinary attachment upload path.
- Destination is captured before the native async operation begins. Delivery
  receives that immutable Chat key (or the new-Chat sentinel); it must not
  re-read the selected Chat after capture completes.
- Delivery switches back to the Orchestrator Chat surface before selecting a
  target or opening the new-Chat canvas, so Workers cannot swallow a capture.
  Capture errors use the same fixed target key as successful delivery.
- `last_appshot_chat` is written only after staging accepts a capture, or when
  a new-Chat Appshot is emitted in the optimistic `Sent` event; deleting that
  Chat clears the remembered target. Staging a capture for another Chat never
  clears a Chat-scoped failure banner.
- Accessibility text is observed, untrusted prompt context. It is XML-escaped,
  bounded, and removed from visible transcript text by clients that present a
  user message. A malformed or ambiguous context is discarded safely.
- `MAX_CAPTURE_*` bounds native allocations and
  `MAX_STAGED_APPSHOT_BYTES` bounds encoded PNG bytes retained by the Composer.
  Permission or backend failure is visible to the active Composer; cancellation
  and self-capture are silent.
- Platform backends may use threads and native event loops, but hand off only
  `CapturedAppshot` values or shortcut events to the UI service.

## Work Guidance

Keep backend-specific APIs inside `macos.rs` or `linux/`. Extend the shared
capture result before adding platform conditionals to the Composer. When
changing destination behavior, test navigation during capture and both the
selected-Chat and new-Chat routes. Preserve the ordinary attachment cache
identity of the screenshot id when changing upload or queued-send handling.

## Verification

| Layer / path | Tier | How to run |
|---|---|---|
| `src/appshots.rs` (limits, XML transport, presentation metadata) | unit | `cargo test -p zeron-ui appshots` |
| `src/appshots/{macos,linux}` (native permission/capture) | none locally; native QA | `scripts/dev-demo.sh`, with platform permissions enabled |
| `src/{shell,composer,lib}.rs` (destination, staging, failure recovery) | unit + compile | `cargo test -p zeron-ui composer` and `cargo check -p zeron-ui` |
| `apps/ios/Zeron/Composer/Appshots.swift` (presentation parser) | unit | `xcodebuild test -project apps/ios/Zeron.xcodeproj -scheme Zeron -destination 'platform=iOS Simulator,name=iPhone 17 Pro'` |

## Child DOX Index

None.
