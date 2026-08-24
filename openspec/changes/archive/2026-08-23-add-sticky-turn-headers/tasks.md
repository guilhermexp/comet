## 1. Sticky turn policy

- [x] 1.1 Add RED→GREEN tests for visible group selection, turn boundaries, source-row duplication, next-turn push, chat switching, and streaming remeasurement.
- [x] 1.2 Add per-chat user geometry and bottom-glued viewport selection without scanning the full transcript on scroll frames.
- [x] 1.3 Preserve the initial own-turn runway and the return-to-bottom handoff.

## 2. Native renderer integration

- [x] 2.1 Paint the selected user row through the existing renderer with namespaced element ids and no list-height contribution.
- [x] 2.2 Preserve attachments, mentions, badges, pending opacity, overflow dialog, theme, cache pruning, and subagent-tab behavior.

## 3. Verification and closeout

- [x] 3.1 Run the full `zeron-ui` suite, workspace check, formatting, diff hygiene, and Impeccable detector.
- [x] 3.2 Rebuild/restart the dev bundle and complete a real OMP visual smoke across scrolling and a turn boundary.
- [x] 3.3 Complete independent review, fix every P0/P1/P2 finding, validate/archive this change, and create the local feature commit without push.
