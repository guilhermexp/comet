# visual

Parent: [../AGENTS.md](../AGENTS.md)

## Purpose

Sandboxed Luau/native visual model, charts, 2D drawing, assets, CPU scenes and shader validation.

## Ownership

Owns this crate's public model and implementation. Hosts own product controls and platform painting.

## Local Contracts

No filesystem/network/process APIs in scripts; bound script bytes, VM memory, call/draw budgets, tree complexity, decoded assets and geometry. Native node parsing charges every visit (including false/empty DAG branches), checks its deadline, and charges retained text before copying (8,000 bytes per text, 1 MiB per view/scene/canvas). This includes axes labels and default x/y/z, numeric layout children, canvas text and independently retained implicit tab values/link labels; defaults are charged before copying just like explicit input. Empty explicit values retain precedence; missing entries in an axes labels table stay empty. Captured print is limited to 8,000 bytes per line and 64 KiB over the runtime lifetime. Model import preflights at most 128 buffers/images and separate 16 MiB logical buffer/image budgets before import/decode; identical image views share encoded backing and retain their indices. Before any attribute/index vector is collected, a pre-pass charges every usable primitive visit (instances and primitives repeat the charge) for positions, readable normals/UVs and explicit or implicit indices with checked arithmetic against 64 MiB per visit and 256 MiB summed over visits, since a base-less sparse accessor expands a tiny file (primitives sharing one large vertex accessor stay usable). Only an absent index accessor yields implicit indices: an explicit one the reader can't open (gltf returns `None` for a view too short for its count) is refused in that pre-pass. Charts, canvases, scenes and shaders share one host surface-id domain: the parser refuses a repeated surface key (initial render and every control rerender); keys on other nodes are unrelated. Seeds are deterministic. Hosts own fetch, visibility and lifetime.

## Work Guidance

Preserve final pinned upstream fixes and existing regressions. Keep workspace dependency versions at the root.

## Verification

| Layer / path | Tier | Command |
| --- | --- | --- |
| `src/**` | unit | `cargo test -p zeron-visual` |
| `tests/native_limits.rs` | integration — scaled DAG/text/log/scene/sparse-model allocation and surface-key regressions; never use full exponential/OOM examples | `cargo test -p zeron-visual --test native_limits` |
| Native integration | e2e — actual desktop/iOS app; Linux GPU tests require a software or hardware adapter | Isolated native smoke; no compilation-only acceptance |

## Child DOX Index

No child docs.
