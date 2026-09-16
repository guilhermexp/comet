# Generate commit message

## Why
Changes has an editable message but no agent-assisted draft, unlike the Monocode reference approved by the user.

## What Changes
Add a wand in the commit input, available with staged files in both Orchestrator and Workers. The owning engine captures only staged changes and uses an installed, enabled isolated text harness. The result fills an editable draft; no staging, commit, push or Chat turn is performed. Show progress and errors, preserve manual edits, and discard responses from a previous context.

## Impact
Additive proto/RPC, engine staged capture and isolated generation, shared Changes UI. Monocode reference: 8e1e3b038bf655fe462bd9ff340e637188d458c4, GitChangesPanel.tsx and gitText.ts (MIT). Native implementation reuses Comet infrastructure.
