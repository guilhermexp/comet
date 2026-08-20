# Workers MCP Tool Cards Design

## Goal

Render Comet Workers MCP calls with the actual action and returned result instead of repeating the provider-qualified `mcp__comet-workers__workers` name.

## Design

The MCP server remains unified as one public `workers` tool. The ACP normalizer recognizes provider-qualified MCP names and converts them to `ToolCall::Mcp`; for the Comet Workers server, the `action` argument becomes the displayed operation, such as `launch_worker`, `list_presets`, or `read_output`.

The document keeps a bounded copy of tool output so the existing expandable transcript card can render invocation first and result second. Large results remain capped by the existing harness and transcript limits. Tool inputs remain sanitized after deriving the safe action name; prompts and other large arguments do not enter the replicated document.

## Success criteria

- A call with title `mcp__comet-workers__workers` and `action=launch_worker` renders as `MCP` / `comet-workers · launch_worker`.
- The expanded card shows the returned JSON or text under the invocation.
- Failed MCP calls retain the destructive state and readable failure output.
- Ordinary unknown tools and non-Workers MCP tools keep their current behavior.
- No MCP server schema change and no additional top-level tools.

