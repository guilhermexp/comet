---
name: unpeel-todos
description: Manage the user's todo list — add, complete, edit, and query todos. Reach for this when the user mentions tasks, todos, or things to remember.
---

# Using unpeel-todos

A file-backed todo list (`~/.unpeel/todos.json`, `--file` overrides).

- Add before editing: `add_todo` creates; there is no upsert.
- Todos are ordered; order is user-owned — never reorder without being asked.
- Status line shows "N open · M done"; completing the last open todo is the
  "done" moment users care about — say so.
- Keep titles short (one line); details belong in the body field.
