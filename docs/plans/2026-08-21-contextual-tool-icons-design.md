# Contextual Tool Icons Design

## Goal

Render the most specific available icon for every transcript tool call across
OMP, Claude Code, Codex, Pi, and any other harness that produces the shared
`ToolCall` contract. Preserve the existing chip layout and runtime behavior.

## Current problem

The transcript renderer selects one monochrome Solar icon per broad `ToolCall`
variant. That leaves executable commands and runtime-native tools visually
generic: every command uses the command glyph and unknown OMP tools use the
widget glyph even when their name or input identifies Git, Python, Node,
browser, search, todo, glob, eval, or a concrete file type.

The app already embeds the complete Material Icon Theme catalog and exposes a
filename resolver for the file browser. Orchestrator.dev also contains a proven
contextual tool-icon resolver. The missing seam is a shared Comet resolver that
connects `ToolCall` data to those existing assets.

## Decision

Add one presentation-only resolver in the shared UI layer. It returns a small
descriptor instead of a raw Solar SVG path:

- a Material catalog asset for commands, files, and recognized tool families;
- a Solar asset for established Comet-only semantics where it remains the
  canonical visual;
- an explicit fallback for genuinely unknown tools.

The transcript chip renderer consumes that descriptor while preserving its
existing 18 px tile, 12 px glyph footprint, spacing, state tint, and disclosure
behavior. No harness adapter, protocol enum, engine lifecycle, or persisted
document shape changes.

## Resolution rules

### Commands

Parse compound shell commands without splitting inside quotes. Inspect command
segments in order and choose the first recognized executable after skipping
wrappers, flags, and environment assignments such as `cd`, `env`, `exec`,
`nohup`, `sudo`, and `time`.

The initial mapping follows the Orchestrator.dev reference and the assets
already embedded by Comet, including:

- shells -> console;
- Bun -> bun;
- Cargo and rustc -> rust;
- Chrome and Chromium -> chrome;
- Deno -> deno;
- Docker -> docker;
- Git and GitHub CLI -> git;
- Go and gofmt -> go;
- Java and javac -> java;
- Node -> nodejs;
- npm and npx -> npm;
- Playwright -> playwright;
- pnpm and pnpx -> pnpm;
- Poetry -> poetry;
- Python, pip, pytest, and versioned variants -> python;
- Ruby -> ruby;
- Swift -> swift;
- Terraform -> terraform;
- uv -> uv;
- Yarn -> yarn.

Unrecognized executables fall back to the console icon.

### Files

Read, write, edit, and patch calls resolve the basename through the existing
Material Icon Theme manifest. Known filenames and extensions therefore render
their native catalog icon while unknown paths receive the catalog file
fallback. File contents and paths are not changed.

### Semantic tools

Search, glob, todo/plan, browser/web, question, eval, agent/subagent, worktree,
terminal, Workers/MCP, and other known names map to their existing Material or
Solar semantic asset. Runtime-native `Unknown` calls are matched
case-insensitively by normalized tool name and, where necessary, bounded input
fields such as `language` or `command`.

Truly unknown calls retain the generic widget/settings fallback so the chip
always remains legible.

## Rendering

Material icons render as their original-color embedded image. Solar fallbacks
retain the current theme tint. Both occupy the same fixed tile and optical
footprint, so the change introduces no layout movement and preserves failure,
running, hover, and expanded states.

## Testing

Implementation follows TDD:

1. Add pure resolver tests and observe them fail against the current generic
   behavior.
2. Cover compound commands, wrappers, quoted separators, executable paths,
   versioned Python, file extensions, OMP-native names, and unknown fallbacks.
3. Add a rendering seam test proving that Material descriptors load their
   embedded asset while Solar fallbacks remain valid.
4. Run focused transcript/UI tests, formatting, workspace checks, and the
   existing harness tests affected by shared presentation types.
5. Build and inspect the dev app once with representative calls, then perform
   one bounded correction pass if needed.

## Non-goals

- No new icon artwork or external dependency.
- No changes to Pi, Claude Code, Codex, ACP, or OMP wire behavior.
- No protocol expansion or migration of persisted sessions.
- No redesign of tool chips, titles, grouping, output, or disclosure.
- No push, release, or promotion to `main` as part of this change.
