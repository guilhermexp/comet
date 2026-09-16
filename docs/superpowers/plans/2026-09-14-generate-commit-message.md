# Generate Commit Message Implementation Plan

**Goal:** Generate editable commit drafts from the Changes panel in both modes.
**Architecture:** A new forwardable RPC authorizes the checkout using the existing gate, captures staged Git data via ProcessRunner, and calls an isolated enabled text harness with a 90-second total deadline. The shared sidebar applies the response only to the originating unchanged draft.
**Tech Stack:** Rust, GPUI, existing harness registry and RPC.
**Spec:** openspec/changes/generate-commit-message/specs/source-control/spec.md

## Global Constraints
Only staged changes; no Git mutation or Chat turn. No new dependency. Preserve device routing and user edits. User approved replication of the Monocode flow.

## Tasks
- [x] Engine: GenerateCommitMessageRequest { cwd }, GeneratedCommitMessage { message }; forwardable RPC deadline 100s; authorize before capture. Reuse capture_git and isolated text collector with explicit instructions. Cap the combined staged summary and patch at 46000 bytes from one index read, response at 16 KiB, total budget 90s. Empty staged diff errors before model startup. Test real staged versus unstaged fixture and JSON parser.
- [x] UI: wand in field, busy indicator, preserve text revision/context. Use shared DetailsSidebar and source_control_params for both modes and relay. Disable repeat generation and mutations while running. Test stale draft decision; inspect native app.
- [ ] Verification: cargo fmt --all; targeted engine, RPC and UI tests; cargo build -p zeron. Update owner contracts and validate/archive OpenSpec. No commit or push requested.
