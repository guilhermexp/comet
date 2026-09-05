## Implementation
- [x] Reproduce existing-Chat selection losing last-used model with a failing regression.
- [x] Remember every explicit model pick and preserve navigation behavior.
- [x] Run focused tests, workspace suite and app build; update DOX and archive.

## Verification
- RED: actual Pickers regression kept openrouter/openai/gpt-6-astra in saved defaults after choosing openai-codex/gpt-6-astra inside an existing Chat.
- GREEN: same regression passes, including navigation and storage reload.
- `cargo test --workspace --locked`: blocked by unrelated untracked `crates/ui/examples/preview_smoke.rs:238` calling nonexistent Window::handle.
- `cargo test --workspace --lib --bins --tests --locked`: 2374 passed, 0 failed, 15 ignored.
- `cargo build -p zeron --locked`: passed.
- rustfmt check and scoped diff whitespace check passed. No native render claim; regression uses GPUI TestAppContext without sending prompts.
