The sessions capability lets an agent inspect and coordinate **your other sessions** — the rest of your fleet. It can read another agent's screen or transcript, wait for it to finish, answer a menu, or send it a prompt. It's the `sessions` tool of [Unpeel MCP](/docs/unpeel-mcp), running locally against your own machine.

<aside class="experimental-callout">
  <p class="experimental-callout__label">Experimental</p>
  <p>Turn Sessions use on in <strong>Settings ▸ Experimental</strong>. Unpeel injects the server into supported agent sessions when they start; already-running sessions pick it up after a restart.</p>
</aside>

## Three rules: read everything, coordinate your group, ask across groups

- **Reading is open.** Any enabled session can list and read any other session across every project.
- **Same-group control is free.** Sessions in the same sidebar group can send text or keys to each other without a dialog. A project root, each plain group, and each worktree are separate groups. Moving a session changes its group immediately.
- **Cross-group writes ask you first.** By default, Unpeel asks before one group types into another. Allowing a caller → target pair remembers that direction until either session is removed; you can revoke it anytime.

The `list` action reports each session's `group_id`, `relation_to_caller` (`self`, `group`, or `other`), and whether it can be controlled without approval.

## Approvals

Settings ▸ Sessions MCP has the live write policy:

- **Ask for approval** — the default; remember approved caller → target pairs.
- **Never allow** — same-group writes only.
- **Always allow** — no cross-group dialogs.

Approvals are directional: A → B does not grant B → A. The pending prompt appears on the Mac and paired phones, and the first answer wins.

## Sessions stay user-created

Agents do not get a general start-session action. You create sessions from a project or group in the sidebar, and every session launched there joins that group. This keeps the collaboration boundary visible and easy to change without creating a hidden agent hierarchy.

With **Let sessions create worktrees** enabled, agents can use `create_worktree` and `list_worktrees` to prepare isolated checkouts. They still cannot launch a session into one; session creation remains yours.

## Actions

| Action | What it does |
| --- | --- |
| `current` | Show the caller, its effective group, peer count, and access |
| `list` | List running sessions with group relationship and control metadata |
| `inspect` | Compact status, screen tail, and transcript tail for one session |
| `read_screen` / `read_output` | Read the rendered terminal or raw output tail |
| `read_transcript` | Read the provider conversation as Markdown |
| `wait_for_status` / `wait_for_text` | Wait for a turn to settle or text to appear |
| `send_text` / `send_keys` | Type into another session; same-group is free, cross-group follows your policy |
| `list_group` / `wait_for_group` / `summarize_group` | Coordinate the other sessions in the caller's group |
| `report_to_group` | Send a structured update to a chosen group peer |
| `list_presets` | List configured launch presets |
| `create_worktree` / `list_worktrees` | Prepare isolated worktrees; off by default |
| `close` | Close another session in the caller's group |

Everything runs locally against session state under `~/.unpeel`; session content is not sent to an Unpeel cloud.

## Guardrails

- Cross-group write approvals are narrow, directional, visible, and revocable.
- Closing is stricter than typing: it is same-group-only and never uses an approval override.
- A session cannot write into or close itself.
- Moving a session into or out of a group is an explicit user action and updates the boundary immediately.
- Legacy parent IDs from older builds remain readable for compatibility but no longer affect layout or access.

Sessions use is for coordinating a fleet of peers: a reviewer watching a builder, several sessions working inside one research group, or a one-off request to another group that you approve. Provider-native subagents remain the right choice for invisible, model-managed delegation.
