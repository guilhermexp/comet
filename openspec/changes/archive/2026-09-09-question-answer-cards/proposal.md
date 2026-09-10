# Question answer cards

## Why
Interactive questions currently render as a generic ask tool plus a passive input chip, and the selected answers are lost from the input history.

## What Changes
- Persist submitted answers additively on input resolution and input parts, including orphan recovery.
- Present question loading, waiting and Answer/Answers cards in the native transcript, without a redundant ask tool row.
- Preserve the existing interactive composer panel and distinguish missing historical answers, skipped requests and errors.

## Impact
Rust proto/doc/engine/UI, harness cancellation constructors, and the matching edge document types. No provider calls, deployment or dependency changes.
