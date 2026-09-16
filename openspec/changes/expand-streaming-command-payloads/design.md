## Context
`tool_detail_default_open` already encodes the whole rule — call kind plus an
`active_group` flag — and `RowKind::ToolGroup` already carries
`detail_auto_open` through the row fingerprint. Both were wired to a constant
`false` by the streaming squash, so no new state is needed.

## Decisions
- `detail_auto_open` follows `entry.status == Streaming`, per entry, not per
  group: the old `last_ix == last_part_ix` clause opened only the trailing group,
  which is not what "acompanhar a saída ao vivo" asks for when the turn already
  ran several commands.
- Drop the `(!resolved || is_last)` clause from `tool_detail_default_open`. A
  command that finished two calls ago is exactly the output the user scrolls
  back to; hiding it again mid-turn was the same defect one chip later.
- Keep `auto_open` (the group fold) false. Groups do not collapse
  (`tool_group_collapses` returns false), so the flag has no user-visible job.
- Nothing else changes: `settle_turn_steps_child` already forces both flags off,
  the fold map keyed by `{row_id}#d{ix}` already wins over the default, and the
  fold tween already refuses to animate auto-opens.

## Risks
- A turn with many commands is much taller while live. Accepted: the user asked
  for it, and settling restores the compact record.
- Each open payload is bounded at `OUTPUT_DETAIL_MAX_LINES` (24) with a 360px
  viewport, so the extra height per chip is bounded even for large outputs.

## File cards
`RowKind::FileChange` had no streaming flag at all, and `file_card_can_expand`
requires `resolved` — so a live card could not open even on a click. It gains an
`auto_open` field, set from the same `streaming` bit and folded into the row
version through `tool_fingerprint`'s existing detail bit, so settling forces the
repaint that closes it.

Two consequences had to be handled rather than inherited:
- The auto-open reads only the doc-resident preview. It must NOT start the
  lazy full-file fetch, which stays an explicit act — `file_card_preview`
  already falls back to the durable preview when `full` is absent.
- The "… N earlier lines" notice was gated on `!open`, which assumed open
  implied a fetched full file. It now also shows whenever the full input is
  absent, or an auto-opened truncated preview would hide its own cut.

The generating viewport stays a FIXED height rather than growing per line: a
content-sized box would reflow the transcript and the sticky turn geometry on
every chunk.
