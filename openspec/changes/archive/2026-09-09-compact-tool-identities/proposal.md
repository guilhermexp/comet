## Why
Skill headers omit the invoked skill, hub headers retain generic execution copy, and diff expansion footers leave empty black bands.

## What Changes
- Show the skill identifier beside Skill and the hub operation/target without Ran hub.
- Remove the separate diff expansion footer; retain expansion on the existing header.

## Impact
Native transcript presentation and the existing optional render input sanitizer in Rust/edge; preserve only Skill identifier fields. No new wire shape or execution changes.
