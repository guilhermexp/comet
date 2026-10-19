# visual-gpu

Parent: [../AGENTS.md](../AGENTS.md)

## Purpose

Offscreen GPU rendering of validated visual scenes and GLSL shaders for desktop and mobile.

## Ownership

Owns this crate's public model and implementation. Hosts own product controls and platform painting.

## Local Contracts

GPU work runs off the UI thread. Reuse bounded targets/state; preserve device loss/errors and release textures/state on teardown. No sync or MCP authority here.

## Work Guidance

Preserve final pinned upstream fixes and existing regressions. Keep workspace dependency versions at the root.

## Verification

| Layer / path | Tier | Command |
| --- | --- | --- |
| `src/**` | unit | `cargo test -p zeron-visual-gpu` |
| Native integration | e2e — actual desktop/iOS app; Linux GPU tests require a software or hardware adapter | Isolated native smoke; no compilation-only acceptance |

## Child DOX Index

No child docs.
