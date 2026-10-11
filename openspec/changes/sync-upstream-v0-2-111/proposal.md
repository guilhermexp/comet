## Why

Integrate the owner's requested upstream refresh and today's fork PRs, then build and install the macOS release without opening it. PR #20 pins upstream `54954e4d22bd4d78b2e2c7160a913b1af548dc79`; PR #21 pins worked-project diff totals at `618039526fcdad81321afded849f95f66702d730`. PR #19 is already in the starting main `e80142973ec72fcf7ed7297d7a0eb7c453485241`.

## What Changes

- Merge all 19 upstream commits since `1074bc54`: native PR board/inspection, searchable menus and ordered sidebar sections, queue/chat mentions, panel/session-card layout, unified workspace/content search, navigation and Providers polish.
- Preserve the fork's Workers, OMP/Kimi, mailbox notifications, MCP grants, Details widgets, terminal placement, markdown hard-break fix and package install without auto-launch.
- Include PR #21's local multi-checkout subscription and exact-cwd diff totals, including its lifecycle and regression tests.
- Update vendored zui from a separate source checkout to the upstream-required API, preserving local patches and both closing-punctuation rules.
- Record conflict decisions, checks, remaining limitations and delivery in the versioned sync history.

## Impact

Affected surfaces: engine source control/search, proto/RPC methods, desktop shell/menus/settings/PR inspection, gpui vendor and macOS packaging. No new Cloudflare credentials, tags or direct upstream publication.
