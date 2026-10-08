## Context

The upstream interval adds cross-platform Codex realtime infrastructure and broad UI changes. Main already includes owner changes and the newly proven direct Worker notification repair. Forty-three paths conflict. The fork uses a vendored Apache zui tree with glass, headless keyboard and punctuation patches, while upstream uses pinned Git dependencies.

## Goals / Non-Goals

Import the full functional interval with ancestry. Preserve independent OMP Live and Codex voice protocols, durable commands and runtime update leases. Preserve fork packaging identity, version 0.2.18 and controlled release feed. Do not reinstall the running app, mutate production Chat data or exercise paid provider calls merely to run offline tests.

## Decisions

1. Isolated sync branch starts at b01350d6; format before a no-commit merge. Integrate disjoint domain resolutions, then verify before main fast-forward.
2. Merge implementations rather than taking ours globally: Codex idle/realtime behavior coexists with OMP Live, Worker delivery and shutdown semantics. All changed call sites receive new protocol fields without dropping fork ones.
3. Repository grouping is presentation identity, not a replacement for checkout/device IDs or Workers repository identity.
4. Import chip references while retaining fork project-file mentions, long paste, Appshots and durable steering semantics. Provider prompts are resolved to plain labels and uploads use existing delivery rails.
5. Reconstruct vendor changes in a separate source checkout, apply the upstream zui delta and retain licensed local patches, then refresh the vendor snapshot and provenance.
6. Root workspace dependencies remain centralized. Adopt new audio/media/session/orb/veil libraries with owner DOX docs and compatible targets. Keep removed Trajectory and duplicate Files/Details navigation removed.
7. Existing workflow gates remain fork-specific. Record all adapted/rejected implementation details in docs/upstream-sync.md, including version bumps, despite importing all functional changes.

## Risks / Trade-offs

Auto-merges can silently omit fork behavior; audit both diffs and test the direct notice, OMP Live and session lifecycle cases. Voice introduces native codecs/device requirements; offline protocol/device fixtures prove mechanics but cannot establish real microphone permission, provider service or remote network quality. Local compilation is expensive and must run serially through cargo-verify. An upstream green run does not verify this adapted revision.

## Migration / Rollback

Additive optional registry and voice fields preserve previous docs. The merge is a local branch commit; main advances only after recorded checks. No production data migration or deployment is performed.

## Accepted interval and adaptations

| Upstream commits | Result | Fork adaptation |
|---|---|---|
| dbb639be #799, 3d4bfd11 #811 | TAKE grouping/filtering, icons and rolling labels | Preserve concrete checkout/device targets, Worker repository identity and fork navigation |
| 73bd3c84 #775, d5c1cdc1 #816 | TAKE inline attachment references and pin geometry | Keep project-relative mentions, long-paste precedence, Appshots and Enter steering |
| 8e6da959 #826 | TAKE iOS failed-open recovery | No production fixture fallback |
| 4a5fb8b4 #819 | TAKE Codex native/remote voice, idle plumbing, iOS activity and packaging | Coexist with OMP Live, root MCP grants, Worker updates/leases and bounded shutdown |
| 970f41fa #835 | TAKE Claude snapshot normalization | Curated rows only; unknown snapshots remain |
| 8930cd10 #834 | TAKE local helper 0.161 request compatibility | Installed standalone helper remains authoritative |
| 16d42a63, 9ac03a54, c8eb7524 | TAKE screenshots, translations and DeepWiki links | Retain fork workflow and trust/profile documentation; identify upstream binaries as upstream |
| 9b377308, 3ef98ec2, 9e83ceea, 916cb1cc | ADAPT upstream version commits | Fork version stays 0.2.18 and no v* tags are imported |

Conflict owners resolve every behavioral hunk; Cargo.lock is resolved by the final manifest graph (1244 packages). zui remains vendored at exact tree 6e4082751218f31cc40f3f2ace860c0556ba5baa with the new glyph delta. The duplicate upstream macos.yml is removed; native chip and orb checks are carried by the fork UI/voice workflows instead. The removed Explorer sections, Trajectory, composer branch footer and upstream release feed remain removed.

## Review-driven adaptations

The fork keeps its durable Steer route for working Chats, including TurnBoundary delivery. The upstream QueueMessage path cannot replace it: it would bypass attachment-transfer registration and mint an ID unrelated to the local echo. Attachment chips resolve over the existing QueueCommand transport and acknowledgement lifecycle. Live-preview syntax highlighting remains test-only as required by the fork's prior performance change; existing paint decoration and code backgrounds remain.

The first core review found a mismatch between upstream voice orchestration instructions and the fork's universally cleared RunRequest.mcp map, plus an unbounded optional account/read warmup before ordinary text startup. The voice coordinator uses the existing engine-owned root authorization path; user prompts do not confer a grant, and the MCP map remains empty. The optional warmup receives a short deadline with a fake-server regression that proves text startup proceeds without an account reply. These are integration repairs, not blanket tool injection or a weakening of update/Worker gates.

The final UI review also found basename-based chip association colliding for two external files, and chip-number reuse when a pending send clears the stage before a failure merges newly staged attachments. A pure proto derivation pairs distinct chip indices with uploads in staging order, preferring exact labels before sanitized matches; desktop queue/transcript and mobile consume the same rule without changing the wire shape. A pending new-Chat send shares its allocation high-water mark between the minted Chat and canvas, including attachments staged on both sides. Completion unlinks that pair; failure restores each draft's fresh chip links with its attachments. Regressions cover queue editing, deletion, preview, undo and navigation between both restoration keys.
