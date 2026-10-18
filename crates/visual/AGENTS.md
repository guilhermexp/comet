# visual

Parent: [../AGENTS.md](../AGENTS.md)

## Purpose

Sandboxed Luau/native visual model, charts, 2D drawing, assets, CPU scenes and shader validation.

## Ownership

Owns this crate's public model and implementation. Hosts own product controls and platform painting.

## Local Contracts

No filesystem/network/process APIs in scripts; bound script bytes, VM memory, call/draw budgets, tree complexity, decoded assets and geometry. Seeds are deterministic. Hosts own fetch, visibility and lifetime.

## Work Guidance

Preserve final pinned upstream fixes and existing regressions. Keep workspace dependency versions at the root.

## Verification

| Layer / path | Tier | Command |
| --- | --- | --- |
| `src/**` | unit | `cargo test -p zeron-visual` |
| Native integration | e2e — actual desktop/iOS app; Linux GPU tests require a software or hardware adapter | Isolated native smoke; no compilation-only acceptance |

## Child DOX Index

No child docs.
