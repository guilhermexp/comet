## Context
launch_worker requires preset_id; read_output/wait_for_status and other Worker operations use session_id. The transcript currently discards preset_id. WorkersModel has preset labels/CLI IDs and session titles/active runtime IDs.

## Decisions
Extend the local equality-guarded header catalog with preset/session labels and existing runtime_icon_path results. Extend the existing header projection to keep project chips and add one icon/name chip. A session target takes precedence over project/preset, and an unknown session never falls back to an unrelated preset. Launch labels identify the selected preset, not a guessed resulting Worker. Catalog refresh updates labels; missing/remote/unknown targets keep technical copy. IDs remain in expanded payloads. Preserve only preset_id in addition to the existing bounded identifier whitelist; never retain briefing or command text. Existing transcripts missing preset_id cannot reconstruct the preset.

Legacy edge materializer was inspected: it already drops all MCP inputs; this change does not modify that legacy path. Native chat2 document transport carries Rust-sanitized parts.

## Verification
TDD for sanitizer preservation/privacy and combined header resolution (project + preset, session precedence, unknown IDs, device guard, rename/icon). Run doc/UI suites and app build; inspect isolated native header.
