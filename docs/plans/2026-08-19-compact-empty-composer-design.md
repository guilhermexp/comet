# Compact empty composer

## Goal

Keep the Orchestrator composer at the same compact one-line height whether a chat already exists or is being created.

## Behavior

- An empty new-chat composer uses the compact 49 px layout.
- Existing chats keep the same compact empty layout.
- A newline, text overflow, or a narrow available input width still expands the composer through the existing flip logic.
- Attachments and multiline content retain the existing expanded sizing and caps.

## Scope

Remove only the new-chat force-expanded override. Preserve the route morph, send controls, footer controls, drafts, and automatic growth behavior.
