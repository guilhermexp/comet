# Tasks

- [x] 1.1 Regressions first: inherit full-config copy, override rejection, origin lineage, origin-only caller/parent refusal, engine `SpawnChat` spawned-parent refusal, snapshot seed `originChatId`.
- [x] 1.2 Add `mode` to `CreateChatArgs`; implement inherit through `SpawnChat`; enforce root checks on both fields; update tool catalog and server instructions.
- [x] 1.3 Engine: refuse spawned parents in `spawn_chat`; correct grant comments; fix snapshot seed lineage.
- [x] 1.4 Retire `comet-sessions`: harness mounts (incl. OMP bridge/routing), binary entry point, crate and workspace membership, fixtures/tests, docs (`CONTEXT.md`, AGENTS). Keep historical `sessions` rendering.
- [x] 2.1 MR4: the server is bound to the granted engine: `EngineInfo.deviceId` must equal the origin device on every connection, including a reconnect; a mismatch creates and runs nothing (unit: wrong engine, reconnect to the wrong engine).
- [x] 2.2 MR5: inherit mode resolves an explicit project on any device the engine reaches, without the parent-device restriction (unit: remote Space).
- [x] 2.3 MR6: inherit mode refuses `model_options` and `modelOptions`, including a present null or empty value, before any side effect (unit).
- [x] 3.1 GR8: the spawned-parent refusal test counts Chats before the refused call and checks no grandchild row after it.
- [x] 1.5 Verify the focused Mac suites: 41 MCP, 3 SpawnChat, 22 harness and 65 doc cases passed; strict OpenSpec validation passed. Creation/config copy and lineage/refusal are covered by the MCP engine-world fixtures and real temporary engine/doc stores, not a provider-backed Chat in the installed app. The current-source native `zeron mcp` subprocess additionally advertised configured/inherit with explicit configured default and refused all four inherit override shapes plus configured kind-chat-with-parent before any connection; the exclusive guard port observed zero connections. IDs, complete JSON and schema were independently verified. Evidence: `.tmp/verify/desktop-corrections/mac/{round2,backend-final}/results.json` and `mac/operational-final/{acceptance.json,mcp/f1f3f5-f7-result.json}`. Per the approved isolation contract, no extra full app/engine/provider or real Chat process was launched.

Final Mac package review: `.tmp/verify/desktop-corrections/final-review-receipt.json` records the actual review mode, reviewed tree and automatic/manual summary path.
