# Inline transcript disclosures

## Why
Event chevrons are pushed to the column edge by growing text slots. Eval labels repeat js despite the JavaScript icon.

## What Changes
- Size event labels to their text, shrinking only when constrained, so chevrons follow the text.
- Remove the redundant JavaScript language prefix from eval presentation in the UI.

## Impact
- crates/ui/src/transcript.rs
- turn-step-tool-groups
