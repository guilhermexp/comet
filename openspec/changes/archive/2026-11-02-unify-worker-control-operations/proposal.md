# Proposal

## Why

Two Worker-control MCP surfaces overlap in reading output, sending text and keys, and judging whether a target can receive input (**F4**): the peer `unpeel.sessions` tool (Workers acting on peers, bounded by group policy and directional approval) and Comet's controller `comet-workers.workers` (the orchestrating Chat acting on its Workers, with task tracking and parent-Chat linkage). They duplicate key encoding, terminal-escape cleanup and the live-target guard, with diverging vocabularies (`ctrl+c` vs `ctrl-c`, `shift+tab` only on one side, `text:` only on the other) and slightly different exited/hibernated handling.

## What Changes

- One shared operation layer in the vendored core (`session_control`) owns the typed Worker-control operations both tools perform: target resolution (`read_target`, `live_target`) and enumeration (`hosted_sessions`), terminal output reads from a cursor (`read_output` with `Tail`/`From`), provider transcript reads under two profiles (`read_transcript` with `Compact` and `Whole`, plus `page_text` for byte paging), the wait vocabulary and verdict (`parse_wait_target`, `judge_wait`), key encoding (superset vocabulary, bounded), ANSI cleanup and the single text/keys delivery path. It holds no policy and no lifecycle verbs.
- Requests carry an explicit authority or profile: `InputAuthority::Peer`/`Controller` for input (only the controller can attach a task receipt), `TranscriptProfile::Compact` (peer low-context tail) or `Whole` (controller, paged at 96 KiB), `WaitVocabulary::Peer` (activity labels, `done`≈`idle`, no task completion) or `Controller` (activity and lifecycle labels plus `completed`, matched only by current-episode evidence).
- Adapters stay separate and keep their own authorization and wire shapes: peer reach/self/group/directional approval and its JSON/text answers; controller authority marker, Workers registry projection, parent-Chat linkage as notification relation, launch/stop/archive authority, and task completion distinct from process idle/exited.
- Peers never gain controller authority, the engine bearer, the controller marker, launch power, Chat tools or lifecycle verbs by this unification; Workers and Chats stay separate. No universal dispatcher is introduced.

## Capabilities

### New Capabilities

- `worker-control-contract`: shared input/read mechanics behind the peer and controller Worker tools, with explicit authority.

## Impact

- Vendored `third_party/unpeel/crates/unpeel-core`: `session_control.rs` (shared operations), `mcp_host.rs` (peer list/inspect/read_output/read_transcript/wait_for_status and the send paths), `controller_host.rs` (bootstrap enumeration, `/mobile/output`, target checks) and `controller_api.rs` (`/mobile/transcript-markdown`), with provenance update; `crates/workers-unpeel/src/controller_mcp.rs` (wait vocabulary/verdict, transcript paging and send paths).
- No wire or persisted-format change; tool argument shapes keep accepting their documented spellings and each tool answers in its existing shape.
