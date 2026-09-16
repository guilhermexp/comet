## Context
`semantic_tool_icon(name, input)` already receives the input, and already reads
an argument to branch for the `hub` tool (`op`). The `workers` branch was folded
into `lower.contains("agent") || lower == "workers"`, which is why it collapsed
to one glyph.

## Decisions
- Solar, not material. The material set is a FILE-TYPE vocabulary rendered as a
  colour image (`img()`); these actions are control-plane verbs, and the row's
  other semantic glyphs that are not file types (`GIT_BRANCH` for worktrees,
  `PEN` for a pathless patch) are already Solar. Solar renders through
  `icons::icon()` tinted with `theme.text_muted`, which is what removes the
  "same green thing on every line" reading, not just the repetition.
- Map the documented vocabulary from the orchestrator's own worker loop
  (`crates/harness/src/omp/mod.rs`) and the activity buckets in
  `turn_steps.rs`, so the icon set and the summary categories cannot drift apart
  silently.
- `launch_worker` keeps the bot glyph: it is the one action that brings a worker
  into existence. The unknown-action fallback is the neutral `WIDGET` instead,
  so "a worker started" and "we do not recognize this action" never share a
  glyph.

## Risks
- A new action added upstream lands on the neutral fallback rather than failing
  loudly. Accepted: an unrecognized action must still paint something, and the
  header text already names it verbatim.
