# Composer Footer Model Controls Design

## Goal

Move the Orchestrator model and effort controls out of the message input and into the footer beside the branch control.

## Layout contract

- The input pill keeps only text, attachments, and the send button.
- The footer keeps checkout information on the left.
- The footer right cluster is ordered `model`, `effort`, `branch`; branch stays at the extreme right.
- Model and effort retain their existing 32px pill styling, labels, icons, hover/open states, and popovers.
- Model and effort remain available for both new and existing chats, including non-git projects; branch appears only when git metadata exists.
- Menus remain end-anchored above their respective triggers.

## Architecture

Extract the current model/effort rendering from `Pickers::render` into a reusable `render_model_controls` method that owns catalog loading, focus behavior, labels, and model/traits overlays. `render_footer` mounts that cluster before the branch control and accepts the active `Window` for picker focus. Composer removes both inline `Pickers` mounts.

## Testing

Add source-contract tests that ensure the composer no longer mounts picker controls inside either expanded or compact input layouts and that the footer owns the model controls before the branch control. Run picker/composer unit tests, workspace check, and native visual validation after rebuilding dev.
