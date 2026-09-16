## Why
`workers` is ONE MCP tool carrying an `action` argument, and the transcript picks
its row icon from the tool NAME. So `list_projects`, `list_presets`,
`add_project`, `launch_worker` and `wait_for_status` all paint the same coloured
robot from the material file-icon set. A column of identical green glyphs
carries no information and reads as decoration, while every neighbouring row
(commands, reads, edits) has a glyph that says what happened.

## What Changes
- The `workers` tool resolves its icon from its `action`, not from its name.
- Those icons come from the Solar set, so they are monochrome and tinted by the
  row's own muted text color, like the existing worktree and patch glyphs —
  instead of a saturated file-type icon repeated down the column.
- An unrecognized or absent action still resolves to a neutral workers glyph; no
  action is invented and no invocation data changes.

## Capabilities
### Modified Capabilities
- `worker-tool-project-chips`: per-action row icon for Workers tool calls.

## Impact
`crates/ui/src/tool_icons.rs` only. No wire, projection, label, chip or
execution change — the header text and the preset/project chips are untouched.
